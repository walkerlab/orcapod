use std::collections::HashMap;

use tokio::task::spawn_blocking;

use crate::{
    core::{pipeline::PipelineJob, util::get},
    uniffi::{error::Result, model::StreamInfo, orchestrator::Orchestrator},
};

use super::{PipelineRun, PipelineRunner};

/// Docker based pipeline runner meant to execute on a single machine
struct DockerPipelineRunner<T: Orchestrator + Clone> {
    orchestrator: T,
    pipeline_runs: Vec<PipelineRun>,
}

impl<T: Orchestrator + Clone> DockerPipelineRunner<T> {
    /// Create a new Docker pipeline runner
    pub const fn new(orchestrator: T) -> Self {
        Self {
            orchestrator,
            pipeline_runs: vec![],
        }
    }

    async fn process_node(
        &self,
        node_key: &str,
        input_map: &HashMap<String, StreamInfo>,
        pipeline_run: &PipelineRun,
    ) -> Result<()> {
        // Get the node from the pipeline
        let node = pipeline_run.pipeline_job.pipeline.get_node(node_key)?;
        // Process the node
        let output_map = node.process(input_map, self.orchestrator.clone())?;

        // Store the output map in the pipeline run, node_outputs
        pipeline_run
            .node_outputs
            .lock()
            .await
            .insert(node_key.to_owned(), output_map.clone());

        // Notify runner that the node has finished processing
        self.process_node_completetion(node_key, pipeline_run);
        Ok(())
    }

    async fn process_node_completetion(
        &self,
        node_key: &str,
        pipeline_run: &PipelineRun,
    ) -> Result<()> {
        let node_outputs = pipeline_run.node_outputs.lock().await?;

        // Find all chidren that depend on the node that was just completed and check if they can be processed
        get(&pipeline_run.pipeline_job.pipeline.edges, node_key)?
            .iter()
            .for_each(|child_key| {
                if pipeline_run
                    .pipeline_job
                    .pipeline
                    .get_parents_key_for_node(child_key)
                    .all(|parent_key| node_outputs.contains_key(parent_key))
                {
                    // Merge all output_maps of parents into one input_map to be passed to the child node
                    let input_map = node_outputs.
                    // All parents are complete, start processing the child node
                    self.process_node(child_key, input_map, pipeline_run)
                }
                // You can add logic here to handle when all_parents_complete is true
            });

        Ok(())
    }
}
