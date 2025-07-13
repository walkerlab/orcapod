use crate::{
    core::{
        graph::{DotAttribute, make_dot},
        pipeline::{NodeInfo, NodeState, Payload},
    },
    uniffi::{
        error::{OrcaError, Result, selector},
        model::{Packet, PipelineJob},
        orchestrator::agent::AgentClient,
    },
};
use chrono::Local;
use derive_more::Display;
use getset::CloneGetters;
use serde::{Deserialize, Serialize};
use snafu::ResultExt as _;
use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
    sync::{Arc, Mutex},
};
use tokio_util::task::TaskTracker;
use uniffi;

// use case

// w/ agent: send remotely and check progress (agent required since it has direct access to orchestrator. agent should be installed/started in whichever deploy strategy makes sense for user)

// new: initialize
// attach: so pipeline run state is updated
// start_pipeline_run: send so pipeline run starts
// summarize: check updates
// get_pipeline_result: long running waiting until pipeline run becomes a pipeline result

/// Status of a particular compute pipeline run.
#[derive(uniffi::Enum, Debug, Serialize, Deserialize)]
pub enum PipelineStatus {
    /// Run has completed successfully.
    Completed,
    /// Run failed.
    Failed,
}

/// Current computational pipeline managed by an orchestrator agent.
#[expect(clippy::field_scoped_visibility_modifiers, reason = "debug")]
#[derive(uniffi::Object, Debug, Display, CloneGetters)]
#[getset(get_clone, impl_attrs = "#[uniffi::export]")]
#[display("{self:#?}")]
#[uniffi::export(Display)]
pub struct PipelineRun {
    /// Original compute request.
    pub pipeline_job: Arc<PipelineJob>,
    #[getset(skip)]
    pub(crate) agent_client: Arc<AgentClient>,
    #[getset(skip)]
    pub(crate) state: Arc<Mutex<HashMap<String, NodeInfo>>>,
    #[getset(skip)]
    pub(crate) services: TaskTracker,
}

#[uniffi::export(async_runtime = "tokio")]
impl PipelineRun {
    /// Initialize a pipeline run instance.
    #[uniffi::constructor]
    pub fn new(pipeline_job: Arc<PipelineJob>, agent_client: Arc<AgentClient>) -> Self {
        Self {
            pipeline_job,
            agent_client,
            state: Arc::new(Mutex::new(HashMap::new())),
            services: TaskTracker::new(),
        }
    }
    /// Start updating the pipeline run state based on network updates.
    ///
    /// # Panics
    ///
    /// Will panic if unable to acquire mut ref on services.
    ///
    /// # Errors
    ///
    /// Will error if there is an issue attaching to the status topic.
    #[expect(
        clippy::expect_used,
        clippy::unused_async,
        clippy::excessive_nesting,
        clippy::indexing_slicing,
        reason = "debug"
    )]
    pub async fn attach(&self) -> Result<()> {
        self.services.spawn({
            let agent_client = Arc::clone(&self.agent_client);
            let pipeline_hash = self.pipeline_job.hash.clone();
            let state = Arc::clone(&self.state);
            let pipeline_metadata = self.pipeline_job.pipeline.metadata.clone();
            async move {
                let subscriber = agent_client
                    .session
                    .declare_subscriber(&format!(
                        "group/{}/status/pipeline_job/{}/**",
                        &agent_client.group, &pipeline_hash
                    ))
                    .await
                    .context(selector::AgentCommunicationFailure {})?;
                // let tracker = TaskTracker::new();
                loop {
                    let sample = subscriber
                        .recv_async()
                        .await
                        .context(selector::AgentCommunicationFailure {})?;
                    // todo: can remove need for this by updating RE_AGENT_KEY_EXPR
                    let subtopics = sample.key_expr().as_str().split('/').collect::<Vec<_>>();
                    let (feed_type, source) = (subtopics[5], subtopics[6]);
                    let payload = serde_json::from_slice::<Payload<Packet, ()>>(
                        &sample.payload().to_bytes(),
                    )?;
                    if feed_type == "output" {
                        let mut inner_state = state.lock().expect("debug");
                        if let Some(node_info) = inner_state.get_mut(source) {
                            let node_state = match &payload {
                                Payload::Cancelled => NodeState::Cancelled,
                                Payload::Failed(error_msg) => {
                                    NodeState::Failed(error_msg.to_owned())
                                }
                                Payload::End(()) => NodeState::Completed,
                                Payload::Stream(_) => node_info.state.clone(),
                            };
                            *node_info = NodeInfo {
                                state: node_state,
                                completed_packets: node_info.completed_packets
                                    + u32::from(matches!(payload, Payload::Stream(_))),
                            };
                        } else {
                            let node_state = match &payload {
                                Payload::Cancelled => NodeState::Cancelled,
                                Payload::Failed(error_msg) => {
                                    NodeState::Failed(error_msg.to_owned())
                                }
                                Payload::End(()) => NodeState::Completed,
                                Payload::Stream(_) => NodeState::Active,
                            };
                            inner_state.insert(
                                source.to_owned(),
                                NodeInfo {
                                    state: node_state,
                                    completed_packets: u32::from(matches!(
                                        payload,
                                        Payload::Stream(_)
                                    )),
                                },
                            );
                        }
                    } else if feed_type == "input" {
                        let mut inner_state = state.lock().expect("debug");
                        if !inner_state.contains_key(source) {
                            inner_state.insert(
                                source.to_owned(),
                                NodeInfo {
                                    state: NodeState::Active,
                                    completed_packets: 0,
                                },
                            );
                        }
                    }
                    let inner_state = state.lock().expect("debug");
                    if inner_state.keys().collect::<HashSet<_>>()
                        == pipeline_metadata.keys().collect::<HashSet<_>>()
                        && !inner_state.values().any(|v| {
                            matches!(v.state, NodeState::Idle)
                                || matches!(v.state, NodeState::Active)
                        })
                    {
                        break;
                    }
                }
                Ok::<_, OrcaError>(())
            }
        });
        Ok(())
    }
    /// Generates summary of compute pipeline status.
    ///
    /// # Panics
    ///
    /// Will panic if unable to acquire mut ref on services.
    #[expect(
        clippy::expect_used,
        clippy::excessive_nesting,
        clippy::let_underscore_must_use,
        reason = "debug"
    )]
    pub fn summarize_dot(&self) -> String {
        let summary_msg = if self.services.is_empty() {
            "Pipeline not active.\n".to_owned()
        } else {
            "Pipeline running...\n".to_owned()
        };
        let mut error_msgs = String::new();
        let node_attributes = &self
            .pipeline_job
            .pipeline
            .metadata
            .keys()
            .map(|node| {
                let node_attribute;
                if let Some(node_info) = self.state.lock().expect("debug").get(node) {
                    node_attribute = (
                        node.clone(),
                        DotAttribute {
                            color: match &node_info.state {
                                NodeState::Idle => todo!("Should not be possible"),
                                NodeState::Cancelled => "chocolate4".into(),
                                NodeState::Completed => "green".into(),
                                NodeState::Active => "yellow2".into(),
                                NodeState::Failed(error_msg) => {
                                    let _ = writeln!(
                                        error_msgs,
                                        "{node}: {}",
                                        error_msg
                                            .split_once("stack backtrace")
                                            .map(|(prefix, _)| prefix)
                                            .map_or(error_msg.as_str(), |brief_error_msg| {
                                                brief_error_msg
                                            })
                                    );
                                    "red".into()
                                }
                            },
                            extra_label: format!("p={}", node_info.completed_packets),
                        },
                    );
                } else {
                    node_attribute = (
                        node.clone(),
                        DotAttribute {
                            color: "black".into(),
                            extra_label: "p=0".into(),
                        },
                    );
                }
                node_attribute
            })
            .collect::<HashMap<_, _>>();

        make_dot(
            &self.pipeline_job.pipeline.graph,
            &self.pipeline_job.pipeline.metadata,
            Some(format!(
                "Pipeline Job Summary [hash={}]\nUpdated: {}\n",
                self.pipeline_job.hash,
                Local::now().format("%Y-%m-%d %H:%M:%S")
            )),
            Some(node_attributes),
            Some(format!(
                "{}Summary: {summary_msg}",
                if error_msgs.is_empty() {
                    error_msgs
                } else {
                    format!("Errors:\n{error_msgs}\n")
                }
            )),
        )
    }
    // /// Generate an SVG snapshot summary of the compute DAG progress.
    // ///
    // /// # Errors
    // ///
    // /// Will fail if unable to generate summary compute DAG SVG.
    // pub fn summarize_svg(&self) -> Result<String> {
    //     make_svg(
    //         &self.pipeline_job.pipeline.graph,
    //         &self.pipeline_job.pipeline.metadata,
    //         Some(format!(
    //             "Pipeline Job Summary [hash={}]",
    //             self.pipeline_job.hash
    //         )),
    //         Some(
    //             &self
    //                 .pipeline_job
    //                 .pipeline
    //                 .metadata
    //                 .keys()
    //                 .map(|node| {
    //                     (
    //                         node.clone(),
    //                         DotAttribute {
    //                             color: "red".into(),
    //                             extra_label: "stuff(1)".into(),
    //                         },
    //                     )
    //                 })
    //                 .collect::<HashMap<_, _>>(),
    //         ),
    //     )
    // }
}
