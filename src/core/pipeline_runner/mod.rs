use std::{collections::HashMap, sync::Arc};

use tokio::{sync::Mutex, task::JoinHandle};

use crate::uniffi::{error::OrcaError, model::StreamInfo};

use super::pipeline::PipelineJob;

pub trait PipelineRunner {
    fn start(&self, pipeline_job: PipelineJob) -> PipelineRun;
}

struct PipelineRun {
    pipeline_job: PipelineJob,
    join_handles: Vec<JoinHandle<Result<(), OrcaError>>>,
    node_outputs: Arc<Mutex<HashMap<String, HashMap<String, StreamInfo>>>>,
}

impl PipelineRun {
    pub fn new(pipeline_job: PipelineJob) -> Self {
        Self {
            pipeline_job,
            join_handles: vec![],
            node_outputs: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

struct PipelineRunInfo;

mod docker;
