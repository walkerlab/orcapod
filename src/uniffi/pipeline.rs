use crate::{
    core::{
        graph::{DOTAttribute, make_dot},
        pipeline::{NodeInfo, NodeState},
    },
    uniffi::{error::Result, model::PipelineJob},
};
use chrono::Local;
use derive_more::Display;
use getset::CloneGetters;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
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
#[derive(uniffi::Enum, Debug, Serialize, Deserialize, Clone)]
pub enum PipelineStatus {
    /// Run has completed successfully.
    Completed,
    /// Run failed.
    Failed,
    /// Run has not finished.
    Running,
}

/// Current computational pipeline managed by an orchestrator agent.
#[expect(clippy::field_scoped_visibility_modifiers, reason = "debug")]
#[derive(uniffi::Object, Debug, Display, CloneGetters, Clone)]
#[getset(get_clone, impl_attrs = "#[uniffi::export]")]
#[display("{self:#?}")]
#[uniffi::export(Display)]
pub struct PipelineRun {
    /// Original compute request.
    pub pipeline_job: Arc<PipelineJob>,
    /// Status of pipeline run.
    #[getset(skip)]
    pub status: Arc<Mutex<PipelineStatus>>,
    #[getset(skip)]
    pub(crate) state: Arc<Mutex<HashMap<String, NodeInfo>>>,
    #[getset(skip)]
    pub(crate) services: TaskTracker,
}

#[uniffi::export]
impl PipelineRun {
    /// # Panics
    #[expect(clippy::unwrap_used, reason = "debug")]
    pub fn status(&self) -> PipelineStatus {
        self.status.lock().unwrap().clone()
    }
    /// Generates summary of compute pipeline status.
    ///
    /// # Panics
    ///
    /// Will panic if unable to acquire mut ref on services.
    ///
    /// # Errors
    ///
    /// Will fail if there is an issue constructing the summary DOT.
    #[expect(
        clippy::unwrap_in_result,
        clippy::excessive_nesting,
        clippy::expect_used,
        clippy::let_underscore_must_use,
        reason = "debug"
    )]
    pub fn summarize_dot(&self) -> Result<String> {
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
                        DOTAttribute {
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
                        DOTAttribute {
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
}
