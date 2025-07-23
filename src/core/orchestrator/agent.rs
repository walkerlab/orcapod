use crate::{
    core::pipeline::{NodeInfo, NodeState, Payload},
    uniffi::{
        error::{OrcaError, Result, selector},
        model::{Packet, PipelineJob},
        orchestrator::agent::{Agent, AgentClient},
        pipeline::{PipelineRun, PipelineStatus},
    },
};
use chrono::{DateTime, Utc};
use futures_util::future::FutureExt as _;
use regex::Regex;
use serde::{Deserialize, Serialize};
use snafu::{OptionExt as _, ResultExt as _};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex},
};
use tokio::{
    sync::mpsc::{self, error::SendError},
    task::JoinSet,
};
use tokio_util::task::TaskTracker;

#[expect(clippy::expect_used, reason = "Valid static regex")]
pub static RE_AGENT_KEY_EXPR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
            ^
            group/
                (?<group>[a-z_\-]+)/
                    (?<action>request|success|failure)/
                        (?<model_type>[a-z_]+)/
                            (?<ref>[0-9a-f]+)/
                                .*?
                                    host/
                                        (?<host>[a-z_]+)/
                                            timestamp/
                                                (?<timestamp>.*?)
            $
            ",
    )
    .expect("Invalid PodJob action regex.")
});

#[expect(
    dead_code,
    reason = "Need to be able to initialize to pass metadata as input."
)]
#[derive(Debug)]
pub struct EventMetadata {
    pub group: String,
    pub action: String,
    pub model_type: String,
    pub r#ref: String,
    pub host: String,
    pub timestamp: DateTime<Utc>,
}

impl AgentClient {
    pub(crate) async fn publish<T>(&self, topic: &str, payload: &T) -> Result<()>
    where
        T: Serialize + Sync + ?Sized,
    {
        Ok(self
            .session
            .put(
                format!(
                    "group/{}/{}/host/{}/timestamp/{}",
                    self.group,
                    topic,
                    self.host,
                    Utc::now().to_rfc3339()
                ),
                &serde_json::to_vec(payload)?,
            )
            .await
            .context(selector::AgentCommunicationFailure {})?)
    }
    /// Send a log message to the agent network.
    ///
    /// # Errors
    ///
    /// Will fail if there is an issue sending the message.
    pub(crate) async fn log(&self, message: &str) -> Result<()> {
        self.publish("log", message).await
    }
    #[expect(
        clippy::excessive_nesting,
        clippy::indexing_slicing,
        clippy::expect_used,
        clippy::cast_sign_loss,
        clippy::significant_drop_tightening,
        reason = "debug"
    )]
    pub(crate) fn new_pipeline_run(&self, pipeline_job: &Arc<PipelineJob>) -> PipelineRun {
        let pipeline_run = PipelineRun {
            pipeline_job: Arc::clone(pipeline_job),
            created: Utc::now().timestamp() as u64,
            terminated: Arc::new(Mutex::new(None)),
            status: Arc::new(Mutex::new(PipelineStatus::Running)),
            state: Arc::new(Mutex::new(HashMap::new())),
            services: TaskTracker::new(),
        };
        pipeline_run.services.spawn({
            let agent_client = Arc::new(self.clone());
            let inner_pipeline_run = pipeline_run.clone();
            async move {
                let pipeline_result = agent_client
                    .get_pipeline_result(inner_pipeline_run.clone().into())
                    .await?;
                let mut terminated = inner_pipeline_run.terminated.lock().expect("debug");
                *terminated = Some(pipeline_result.terminated);
                let mut status = inner_pipeline_run.status.lock().expect("debug");
                *status = pipeline_result.status;
                Ok::<_, OrcaError>(())
            }
        });
        pipeline_run.services.spawn({
            let agent_client = Arc::new(self.clone());
            let pipeline_hash = pipeline_job.hash.clone();
            let state = Arc::clone(&pipeline_run.state);
            let pipeline_nodes = pipeline_job
                .pipeline
                .graph
                .node_weights()
                .map(|node| node.name.clone())
                .collect::<HashSet<_>>();
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
                    if inner_state.keys().cloned().collect::<HashSet<_>>() == pipeline_nodes
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
        pipeline_run
    }
}

#[expect(
    clippy::excessive_nesting,
    clippy::let_underscore_must_use,
    reason = "`result::Result<(), SendError<_>>` is the only uncaptured result since it would mean we can't transmit results over mpsc."
)]
pub async fn start_service<
    RequestF,  // function to run on requests
    RequestI,  // input to the function for requests
    RequestR,  // output to the function for requests
    ResponseF, // function to run on completing a request i.e. response
    ResponseI, // input to the function for responses
    ResponseR, // output to the function for responses
>(
    agent: Arc<Agent>,
    request_key_expr: String,
    namespace_lookup: HashMap<String, PathBuf>,
    request_task: RequestF,
    response_task: ResponseF,
) -> Result<()>
where
    RequestI: for<'serde> Deserialize<'serde> + Send + 'static,
    RequestF: FnOnce(Arc<Agent>, HashMap<String, PathBuf>, EventMetadata, RequestI) -> RequestR
        + Clone
        + Send
        + 'static,
    RequestR: Future<Output = Result<ResponseI>> + Send + 'static,
    ResponseI: Send + 'static,
    ResponseF: Fn(Arc<AgentClient>, ResponseI) -> ResponseR + Send + 'static,
    ResponseR: Future<Output = Result<()>> + Send + 'static,
{
    agent
        .client
        .log(&format!("Started `{request_key_expr}` service."))
        .await?;
    let (response_tx, mut response_rx) = mpsc::channel(100);

    let mut services = JoinSet::new();
    services.spawn({
        let inner_agent = Arc::clone(&agent);
        async move {
            let tasks = TaskTracker::new();
            let subscriber = inner_agent
                .client
                .session
                .declare_subscriber(format!(
                    "group/{}/{}",
                    inner_agent.client.group, request_key_expr
                ))
                .await
                .context(selector::AgentCommunicationFailure {})?;
            while let Ok(sample) = subscriber.recv_async().await {
                if let (Ok(input), Some(metadata)) = (
                    serde_json::from_slice::<RequestI>(&sample.payload().to_bytes()),
                    RE_AGENT_KEY_EXPR.captures(sample.key_expr().as_str()),
                ) {
                    let inner_response_tx = response_tx.clone();
                    let event_metadata = EventMetadata {
                        group: metadata["group"].to_string(),
                        action: metadata["action"].to_string(),
                        model_type: metadata["model_type"].to_string(),
                        r#ref: metadata["ref"].to_string(),
                        host: metadata["host"].to_string(),
                        timestamp: DateTime::parse_from_rfc3339(&metadata["timestamp"])?.into(),
                    };
                    tasks.spawn({
                        let inner_request_task = request_task.clone();
                        let inner_inner_agent = Arc::clone(&inner_agent);
                        let inner_namespace_lookup = namespace_lookup.clone();
                        async move {
                            inner_request_task(
                                inner_inner_agent,
                                inner_namespace_lookup,
                                event_metadata,
                                input,
                            )
                            .then(move |response| async move {
                                let _: Result<(), SendError<Result<ResponseI>>> =
                                    inner_response_tx.send(response).await;
                                Ok::<_, OrcaError>(())
                            })
                            .await
                        }
                    });
                }
            }
            Ok(())
        }
    });
    services.spawn(async move {
        while let Some(response) = response_rx.recv().await {
            response_task(Arc::clone(&agent.client), response?).await?;
        }
        Ok(())
    });

    services
        .join_next()
        .await
        .context(selector::NoRemainingServices {})??
}
