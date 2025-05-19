use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

use crate::uniffi::{error::Result, model::Input};

use super::pipeline::PipelineJob;
use std::fmt;
use std::hash::{Hash, Hasher};

/// # Errors:
/// Error out if fail to start the pipeline job
pub trait PipelineRunner {
    /// Starts the given pipeline job.
    ///
    /// # Errors
    /// Returns an error if the pipeline job fails to start.
    fn start(&self, pipeline_job: PipelineJob) -> Result<()>;
}

#[derive(Debug)]
/// Struct to store the active pipeline run.
/// Currently only store the `node_outputs` as a form a memory cache.
pub struct PipelineRun {
    pipeline_job: PipelineJob,
    node_outputs: Arc<RwLock<HashMap<String, HashMap<String, Input>>>>,
}

impl fmt::Display for PipelineRun {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PipelineRun {{ pipeline_job: {} }}",
            self.pipeline_job.hash
        )
    }
}

impl PipelineRun {
    /// New function to initialize the pipeline run
    pub fn new(pipeline_job: PipelineJob) -> Self {
        Self {
            pipeline_job,
            node_outputs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn process_node(
        self: Arc<Self>,
        node_key: String,
        input_map: HashMap<String, Input>,
    ) -> Result<String> {
        // Get the node from the pipeline
        let node = self.pipeline_job.pipeline.get_node(&node_key)?.to_owned();
        // Process the node
        let output_map = node.process(&input_map)?;

        // Insert the output map into the pipeline run
        self.node_outputs
            .write()
            .await
            .insert(node_key.clone(), output_map);

        Ok(node_key)
    }

    /// Find all children that depends on the `node_key` and find out which one can be started
    /// by checking if all parents have an output stored in the `node_outputs`
    /// returns a `HashMap` of the `node_keys` and their parents
    #[expect(
        clippy::unwrap_used,
        reason = "The iterator already checks for the key before calling unwrap. Should never panic"
    )]
    async fn get_ready_to_start_children(
        &self,
        node_key: &str,
    ) -> HashMap<String, HashMap<String, Input>> {
        let node_outputs = self.node_outputs.read().await;

        self.pipeline_job
            .pipeline
            .edges
            .get(node_key)
            .as_ref()
            .map_or_else(HashMap::new, |child_nodes_key| {
                child_nodes_key
                    .iter()
                    .filter_map(|child_key| {
                        self.pipeline_job
                            .pipeline
                            .get_parents_key_for_node(child_key)
                            .all(|parent_key| node_outputs.contains_key(parent_key))
                            .then_some({
                                let parents = self
                                    .pipeline_job
                                    .pipeline
                                    .get_parents_key_for_node(child_key);
                                // Get the outputs for parents then combine them into a single hashmap
                                let input_map_for_node = parents
                                    .flat_map(|parent_key| node_outputs.get(parent_key).unwrap())
                                    .map(|(k, v)| (k.clone(), v.clone()))
                                    .collect::<HashMap<String, Input>>();
                                (child_key.clone(), input_map_for_node)
                            })
                    })
                    .collect::<HashMap<String, HashMap<String, Input>>>()
            })
    }
}

impl PartialEq for PipelineRun {
    fn eq(&self, other: &Self) -> bool {
        self.pipeline_job.hash == other.pipeline_job.hash
    }
}

impl Eq for PipelineRun {}

impl Hash for PipelineRun {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.pipeline_job.hash.hash(state);
    }
}

/// Docker pipeline runner
pub mod docker;
