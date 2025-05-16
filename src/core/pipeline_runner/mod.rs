use std::{collections::HashMap, sync::Arc};

use tokio::{net::unix::pipe, sync::RwLock};

use crate::uniffi::{error::Result, model::StreamInfo};

use super::{pipeline::PipelineJob, util::get};

pub trait PipelineRunner {
    fn start(&self, pipeline_job: PipelineJob) -> PipelineRun;
}

struct PipelineRun {
    pipeline_job: PipelineJob,
    join_handles: Arc<RwLock<Vec<tokio::task::JoinHandle<()>>>>,
    node_outputs: Arc<RwLock<HashMap<String, HashMap<String, StreamInfo>>>>,
}

impl PipelineRun {
    pub fn new(pipeline_job: PipelineJob) -> Self {
        Self {
            pipeline_job,
            node_outputs: Arc::new(RwLock::new(HashMap::new())),
            join_handles: todo!(),
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
        for node in children_node_to_start {
            let input_map = self
                .node_outputs
                .read()
                .await
                .get(&node_key)
                .unwrap()
                .clone();

            let self_clone = Arc::clone(&self);
            tokio::spawn(async move {
                self_clone.process_node(node, input_map);
            });
        }

        Ok(())
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

struct PipelineRunInfo;

mod docker;
