use std::{backtrace::Backtrace, collections::HashMap, sync::Arc};

use futures_util::future::join_all;
use tokio::{sync::RwLock, task::JoinHandle};

use crate::uniffi::{
    error::{Kind, OrcaError, Result},
    model::StreamInfo,
};

use super::{pipeline::PipelineJob, util::get};

pub trait PipelineRunner {
    fn start(&self, pipeline_job: PipelineJob) -> Result<()>;
}

struct PipelineRun {
    pipeline_job: PipelineJob,
    node_outputs: Arc<RwLock<HashMap<String, HashMap<String, StreamInfo>>>>,
}

impl PipelineRun {
    pub fn new(pipeline_job: PipelineJob) -> Self {
        Self {
            pipeline_job,
            node_outputs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn process_node(
        self: Arc<Self>,
        node_key: String,
        input_map: HashMap<String, StreamInfo>,
    ) -> Result<()> {
        // Get the node from the pipeline
        let node = self.pipeline_job.pipeline.get_node(&node_key)?.to_owned();
        // Process the node
        let output_map = node.process(&input_map)?;

        // Insert the output map into the pipeline run
        self.node_outputs
            .write()
            .await
            .insert(node_key.clone(), output_map);

        // Find out what children can be started
        let children_node_to_start = self.find_ready_to_start_children_node(&node_key).await?;

        // Process the children nodes
        let mut futures = Vec::with_capacity(children_node_to_start.len());
        let mut child_key_and_input_map = Vec::with_capacity(children_node_to_start.len());
        for child_key in children_node_to_start {
            // Build input_map_for_child from all parent outputs
            let input_map_for_child = {
                let node_outputs = self.node_outputs.read().await;
                let parent_keys = self
                    .pipeline_job
                    .pipeline
                    .get_parents_key_for_node(&child_key);
                parent_keys
                    .filter_map(|parent_key| node_outputs.get(parent_key))
                    .flat_map(|parent_output_map| parent_output_map.clone().into_iter())
                    .collect::<HashMap<String, StreamInfo>>()
            };

            child_key_and_input_map.push((child_key, input_map_for_child));
        }

        for (child_key, input_map_for_child) in child_key_and_input_map {
            // Spawn a new task for each child node
            futures.push(Self::start_node(
                self.clone(),
                child_key,
                input_map_for_child,
            ));
        }

        // Wait for all children to finish
        let results = join_all(futures).await;
        Ok(())
    }

    fn start_node(
        self: Arc<Self>,
        node_key: String,
        input_map: HashMap<String, StreamInfo>,
    ) -> JoinHandle<Result<()>> {
        // Spawn a new task for the node

        tokio::spawn(async move { self.process_node(node_key, input_map).await })
    }

    async fn find_ready_to_start_children_node(&self, node_key: &str) -> Result<Vec<String>> {
        // Find all children that depends on the node_key being completed and check if all of its inputs are ready
        // Get lock so the data doesn't change while we are processing
        let node_outputs = self.node_outputs.read().await;
        let ready_children = get(&self.pipeline_job.pipeline.edges, node_key)?
            .iter()
            .filter_map(|child_key| {
                self.pipeline_job
                    .pipeline
                    .get_parents_key_for_node(child_key)
                    .all(|parent_key| node_outputs.contains_key(parent_key))
                    .then_some(child_key.clone())
            })
            .collect();
        Ok(ready_children)
    }
}

mod docker;
