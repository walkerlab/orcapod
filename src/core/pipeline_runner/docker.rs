use snafu::OptionExt as _;
use tokio::task::JoinSet;

use super::PipelineRun;
use crate::{
    core::pipeline::PipelineJob,
    uniffi::{
        error::{Result, selector},
        model::Input,
    },
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

/// Docker based pipeline runner meant to execute on a single machine
#[derive(Default)]
pub struct DockerPipelineRunner {
    pipeline_runs: HashSet<Arc<PipelineRun>>, // For each pipeline run, we have a join set to track the tasks and wait on them
}

impl DockerPipelineRunner {
    /// Create a new Docker pipeline runner
    pub fn new() -> Self {
        Self::default()
    }

    /// # Errors
    /// Will error out if the pipeline job fails to start
    pub async fn start(&mut self, pipeline_job: PipelineJob) -> Result<()> {
        // Create a new pipeline run
        let pipeline_run = Arc::new(PipelineRun::new(pipeline_job));

        // Insert the pipeline run into the pipeline runs map
        self.pipeline_runs.insert(pipeline_run.clone());

        // Run the pipeline runner
        self.start_pipeline_run_task(pipeline_run).await?;

        Ok(())
    }

    /// # Errors
    /// Will error out if any of the tasks fails
    pub async fn start_pipeline_run_task(&mut self, pipeline_run: Arc<PipelineRun>) -> Result<()> {
        // Get the input_map from pipeline_job
        let input_map = pipeline_run.pipeline_job.input_map.clone();

        // Determine if it is a file/folder or collection, if collection, then we need to split it up into multiple_input_task
        // Find keys that are collections
        let collection_keys: Vec<String> = input_map
            .iter()
            .filter(|(_, input)| match input {
                Input::Unary(_) => false,
                Input::Collection(_) => true,
            })
            .map(|(key, _)| key.clone())
            .collect();

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
