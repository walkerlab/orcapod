use serde::Serialize;
use std::{
    backtrace::Backtrace,
    collections::{HashMap, HashSet},
};
use tokio::{sync::RwLock, task::JoinHandle};

use crate::uniffi::{
    error::{Kind, OrcaError, Result},
    model::{Annotation, Input, Mapper, Pod},
};

use crate::core::model::serialize_hashmap;

#[derive(Serialize, PartialEq, Debug, Clone)]
/// Enum to store different types of nodes explicitly
pub enum Node {
    /// Pod node
    Pod(Box<Pod>),
    /// Mapper node
    Mapper(Mapper),
}

impl Node {
    /// Get the hash of the node
    pub fn get_hash(&self) -> String {
        match self {
            Self::Pod(pod) => pod.hash.clone(),
            Self::Mapper(mapper) => mapper.hash.clone(),
        }
    }
}

impl From<Pod> for Node {
    fn from(pod: Pod) -> Self {
        Self::Pod(Box::new(pod))
    }
}
impl From<Mapper> for Node {
    fn from(mapper: Mapper) -> Self {
        Self::Mapper(mapper)
    }
}

/// Pipeline struct
#[derive(Serialize, Debug, PartialEq, Default)]
pub struct Pipeline {
    hash: String,
    #[serde(skip)]
    annotation: Option<Annotation>,
    nodes: HashMap<String, Node>,   // String are hashes of the nodes
    edges: HashMap<String, String>, // Strings are hashes of the nodes
    output_nodes: HashSet<String>,
}

impl Pipeline {
    /// Creates a new `Pipeline` instance.
    pub const fn new(
        nodes: HashMap<String, Node>,
        edges: HashMap<String, String>,
        output_nodes: HashSet<String>,
        annotation: Option<Annotation>,
    ) -> Self {
        Self {
            hash: String::new(), // TODO: Need to implement to yaml then hash that
            nodes,
            edges,
            output_nodes,
            annotation,
        }
    }

    /// Function to get the root nodes of the pipeline
    pub fn get_root_nodes(&self) -> Vec<&Node> {
        // Return a nodes with degree of 0
        self.edges
            .iter()
            .filter_map(|(key, value)| {
                if value.is_empty() {
                    self.nodes.get(key)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Function to get the leaf nodes of the pipeline
    /// Mainly used to get the output nodes when user does not specify them
    fn get_leaf_nodes(&self) -> Vec<&Node> {
        // Return a nodes with degree of 0
        self.edges
            .iter()
            .filter_map(|(key, value)| {
                if value.is_empty() {
                    self.nodes.get(key)
                } else {
                    None
                }
            })
            .collect()
    }
}

impl From<PipelineBuilder> for Pipeline {
    fn from(val: PipelineBuilder) -> Self {
        let output_nodes: HashSet<String> = if val.pipeline.output_nodes.is_empty() {
            // If there are no output nodes, then we need to set the output nodes to the leaf nodes
            val.pipeline
                .get_leaf_nodes()
                .iter()
                .map(|node| node.get_hash())
                .collect()
        } else {
            val.pipeline.output_nodes
        };

        Self::new(
            val.pipeline.nodes,
            val.pipeline.edges,
            output_nodes,
            val.pipeline.annotation,
        )
    }
}

#[derive(Serialize, Debug)]
/// `PipelineJob` struct
/// This struct is used to store the pipeline and the input map
pub struct PipelineJob {
    pipeline: Pipeline,
    #[serde(serialize_with = "serialize_hashmap")]
    input_map: HashMap<String, Input>,
    annotation: Option<Annotation>,
}

impl PipelineJob {
    /// New function for pipeline job
    /// # Errors
    /// Error out if there are missing keys or failed to convert to yaml
    pub fn new(
        pipeline: Pipeline,
        input_map: HashMap<String, Input>,
        annotation: Option<Annotation>,
    ) -> Result<Self> {
        // Check if input_map has all the requires keys
        let missing_keys = pipeline
            .get_root_nodes()
            .iter()
            .flat_map(|node| match node {
                Node::Pod(pod) => find_missing_keys(&input_map, pod.input_stream.keys()),
                Node::Mapper(mapper) => find_missing_keys(&input_map, mapper.mapping.keys()),
            })
            .collect::<Vec<_>>();

        if !missing_keys.is_empty() {
            return Err(OrcaError {
                kind: Kind::MissingStreamKey {
                    input_map,
                    missing_keys,
                    backtrace: Some(Backtrace::capture()),
                },
            });
        }

        Ok(Self {
            pipeline,
            input_map,
            annotation,
        })
    }
}

fn find_missing_keys<'a>(
    input_map: &HashMap<String, Input>,
    keys_to_check: impl Iterator<Item = &'a String>,
) -> Vec<String> {
    keys_to_check
        .filter_map(|key| {
            if input_map.contains_key(key) {
                None
            } else {
                Some(key.clone())
            }
        })
        .collect()
}

// struct PipelineResult {
//     result: Vec<PodResult>,
// }

trait PipelineRun {
    fn get_join_handles(&self) -> &RwLock<Vec<JoinHandle<()>>>;
}

pub struct PipelineBuilder {
    pipeline: Pipeline,
}

impl Default for PipelineBuilder {
    fn default() -> Self {
        Self {
            pipeline: Pipeline {
                hash: String::new(),
                annotation: None,
                nodes: HashMap::new(),
                edges: HashMap::new(),
                output_nodes: HashSet::new(),
            },
        }
    }
}

impl PipelineBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add nodes to the pipeline and return key to put in edges
    ///
    /// Cases:
    /// 1. If the node is not in the pipeline.nodes, then it is added to the `hash_map` and the key is the node hash
    /// 2. If the node is already in the pipeline.nodes, then the key is the hash + _{`num_matches`} to prevent collision
    pub fn add_node(&mut self, node: impl Into<Node>) -> NodeHandle<'_> {
        let node = node.into();
        let hash = node.get_hash();

        // Insert into node hash_map if does not exist
        self.pipeline.nodes.entry(hash.clone()).or_insert(node);

        NodeHandle {
            node_key: self.get_node_key(&hash),
            pipeline_builder: self,
        }
    }

    fn add_edge_from_node(&mut self, from: String, node: impl Into<Node>) -> NodeHandle {
        // Check if node exists in the pipeline.nodes
        let node = node.into();
        let hash = node.get_hash();

        // Insert into node hash_map if does not exist
        self.pipeline.nodes.entry(hash.clone()).or_insert(node);

        // Get the node_key to add to the edge
        let node_key = self.get_node_key(&hash);
        // Add the edge
        self.pipeline.edges.insert(from, node_key.clone());

        NodeHandle {
            node_key,
            pipeline_builder: self,
        }
    }

    fn get_node_key(&self, node_hash: &str) -> String {
        // Check if node is already in the pipeline, if so then we need to add a numerator to the hash
        let num_matches = self
            .pipeline
            .nodes
            .iter()
            .filter(|(key, _)| *key == node_hash)
            .count();

        if num_matches > 0 {
            // Node already exists, thus we need to add a numerator to the hash
            format!("{node_hash}_{num_matches}")
        } else {
            node_hash.to_owned()
        }
    }
}

pub struct NodeHandle<'a> {
    node_key: String,
    pipeline_builder: &'a mut PipelineBuilder,
}

impl NodeHandle<'_> {
    pub fn add_child(&mut self, node: impl Into<Node>) -> NodeHandle<'_> {
        self.pipeline_builder
            .add_edge_from_node(self.node_key.clone(), node)
    }
}
