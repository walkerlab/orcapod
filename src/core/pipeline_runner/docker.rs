use std::collections::HashMap;

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
