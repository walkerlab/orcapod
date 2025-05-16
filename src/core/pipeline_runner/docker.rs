use futures_util::Stream;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

use crate::{
    core::util::get,
    uniffi::{error::Result, model::StreamInfo},
};

use super::PipelineRun;

/// Docker based pipeline runner meant to execute on a single machine
struct DockerPipelineRunner {
    pipeline_runs: HashMap<usize, PipelineRun>,
}

impl DockerPipelineRunner {
    /// Create a new Docker pipeline runner
    pub fn new() -> Self {
        Self {
            pipeline_runs: HashMap::new(),
        }
    }
}
