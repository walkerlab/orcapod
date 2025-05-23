use petgraph::visit::Bfs;
use snafu::OptionExt as _;
use tokio::{
    sync::broadcast::{self, Receiver},
    task::JoinSet,
};

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
    pipeline_runs: HashMap<Arc<PipelineRun>, HashMap<String, Receiver<String>>>, // For each pipeline run, we have a join set to track the tasks and wait on them
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

        self.pipeline_runs
            .insert(pipeline_run.clone(), HashMap::new());

        // Run the pipeline runner
        self.start_pipeline_run_task(pipeline_run).await?;

        Ok(())
    }

    /// # Errors
    /// Will error out if any of the tasks fails
    pub async fn start_pipeline_run_task(&mut self, pipeline_run: Arc<PipelineRun>) -> Result<()> {
        // TODO: Batch implementation

        // Create source channel queue
        let (tx, mut rx) = broadcast::channel::<HashMap<String, Input>>(1);

        let graph = &pipeline_run.pipeline_job.pipeline.graph;

        // Go through the graph from the leaf node and create all the tasks and channels
        Ok(())
    }

    fn create_task_for_node(
        &mut self,
        node_key: String,
        pipeline_run: &PipelineRun,
    ) -> Receiver<HashMap<String, Input>> {
        // Get parents for the node
        pipeline_run
            .pipeline_job
            .pipeline
            .get_parents_key_for_node(node_key)
            .map(|parent_node_key| {
                // Check if it exists in the pipeline_runs hashmap
                match self
                    .pipeline_runs
                    .get(pipeline_run)
                    .unwrap()
                    .get(&parent_node_key)
                {
                    Some(rx) => rx,
                    None => {
                        // Missing parent node, thus recuvrsively create the task for the parent node
                        self.create_task_for_node(parent_node_key, pipeline_run)
                    }
                }
            })
    }

    fn start_node_task_manager(
        &mut self,
        node_key: String,
        input_map: HashMap<String, Input>,
        rx: Receiver<HashMap<String, Input>>,
    ) -> Result<()> {
        Ok(())
    }
}
