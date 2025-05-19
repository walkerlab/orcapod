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
use std::{collections::HashMap, sync::Arc};

/// Docker based pipeline runner meant to execute on a single machine
#[derive(Default)]
pub struct DockerPipelineRunner {
    pipeline_runs: HashMap<Arc<PipelineRun>, JoinSet<Result<String>>>, // For each pipeline run, we have a join set to track the tasks and wait on them
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
        self.pipeline_runs
            .insert(pipeline_run.clone(), JoinSet::new());

        // Run the pipeline runner
        self.start_pipeline_run_task(pipeline_run).await?;

        Ok(())
    }

    /// # Errors
    /// Will error out if any of the tasks fails
    pub async fn start_pipeline_run_task(&mut self, pipeline_run: Arc<PipelineRun>) -> Result<()> {
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
        while let Some(result) = self
            .pipeline_runs
            .get_mut(&pipeline_run)
            .context(selector::KeyMissing {
                key: pipeline_run.to_string(),
            })?
            .join_next()
            .await
        {
            // Check if the task was successful
            match result? {
                Ok(node_key) => {
                    // Get the Hashmap of children that can start along with their inputs and start them
                    let nodes_to_start = pipeline_run
                        .get_ready_to_start_children(&node_key.clone())
                        .await;

                    // Start each node by spawning a new task with start_node function
                    for (child_node_key, child_input_map) in &nodes_to_start {
                        if let Some(join_set) = self.pipeline_runs.get_mut(&pipeline_run) {
                            let node_key_clone = child_node_key.clone();
                            let input_map_clone = child_input_map.clone();
                            let pipeline_run_clone = pipeline_run.clone();
                            join_set.spawn(async move {
                                pipeline_run_clone
                                    .process_node(node_key_clone, input_map_clone)
                                    .await
                            });
                        }
                    }
                }
                Err(e) => {
                    return Err(e);
                }
            }
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
