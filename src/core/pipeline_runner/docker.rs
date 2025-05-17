use snafu::OptionExt as _;
use tokio::task::JoinSet;

use super::PipelineRun;
use crate::uniffi::{
    error::{Result, selector},
    model::Input,
};
use std::{collections::HashMap, sync::Arc};

/// Docker based pipeline runner meant to execute on a single machine
struct DockerPipelineRunner {
    pipeline_runs: HashMap<Arc<PipelineRun>, JoinSet<Result<String>>>, // For each pipeline run, we have a join set to track the tasks and wait on them
}

impl DockerPipelineRunner {
    /// Create a new Docker pipeline runner
    pub fn new() -> Self {
        Self {
            pipeline_runs: HashMap::new(),
        }
    }

    pub async fn start(&mut self, pipeline_run: Arc<PipelineRun>) -> Result<()> {
        // Get the input_map from pipeline_job
        let input_map = pipeline_run.pipeline_job.input_map.clone();

        // Start all the root nodes
        pipeline_run
            .pipeline_job
            .pipeline
            .get_root_nodes()
            .map(|node_key| {
                self.start_node(node_key.clone(), input_map.clone(), pipeline_run.clone())
            })
            .collect::<Result<Vec<_>>>()?;

        // Wait for finished tasks and schedule their children if all the inputs are ready
        while let (Some(result)) = self
            .pipeline_runs
            .get_mut(&pipeline_run)
            .context(selector::KeyMissing {
                key: pipeline_run.to_string(),
            })?
            .join_next()
            .await
        {
            // Check if the task was successful
        }

        Ok(())
    }

    fn start_node(
        &mut self,
        node_key: String,
        input_map: HashMap<String, Input>,
        pipeline_run: Arc<PipelineRun>,
    ) -> Result<()> {
        // Spawn the task to process the node
        self.pipeline_runs
            .get_mut(&pipeline_run)
            .context(selector::KeyMissing {
                key: pipeline_run.to_string(),
            })?
            .spawn(async move {
                pipeline_run
                    .process_node(node_key.clone(), input_map.clone())
                    .await
            });

        Ok(())
    }
}
