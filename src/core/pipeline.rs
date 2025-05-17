use serde::Serialize;
use std::{
    backtrace::Backtrace,
    collections::{HashMap, HashSet},
};

use crate::uniffi::{
    error::{Kind, OrcaError, Result},
    model::{Annotation, Input, Mapper, Pod, StreamInfo},
};
use std::collections::hash_map::Entry;

use crate::core::model::serialize_hashmap;

use super::util::get;

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

    /// # Errors
    /// Error out if fail to join all parents futures
    pub fn process(&self, input_map: &HashMap<String, Input>) -> Result<HashMap<String, Input>> {
        match self {
            Self::Pod(pod) => {
                // Print out pod hash for now
                println!("Processing pod: {}", pod.hash);
                // TODO: Actual pod job creation and submission to orchestrator
                Ok(input_map.clone())
            }
            Self::Mapper(mapper) => {
                // Print out mapper hash for now
                println!("Processing mapper: {}", mapper.hash);
                Ok(input_map.clone())
            }
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
#[derive(Serialize, Debug, PartialEq, Default, Clone)]
pub struct Pipeline {
    hash: String,
    #[serde(skip)]
    annotation: Option<Annotation>,
    pub nodes: HashMap<String, Node>, // String are hashes of the nodes without the _{num_matches}
    pub edges: HashMap<String, Vec<String>>, // Strings are hashes of the nodes
    output_nodes: HashSet<String>,
}

impl Pipeline {
    /// Creates a new `Pipeline` instance.
    pub const fn new(
        nodes: HashMap<String, Node>,
        edges: HashMap<String, Vec<String>>,
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

    /// # Errors
    /// Error out if the `node_key` is not found in the pipeline.nodes
    pub fn get_node(&self, node_key: &str) -> Result<&Node> {
        get(&self.nodes, node_key.trim_end_matches('_'))
    }

    /// Function to get the root nodes of the pipeline
    pub fn get_root_nodes(&self) -> impl Iterator<Item = &String> {
        // Root nodes are those that are not values in the edges map (i.e., not children of any node)
        self.edges
            .keys()
            .filter(move |k| !self.edges.values().any(|v| v.contains(*k)))
    }

    /// Function to get the leaf nodes of the pipeline
    /// Mainly used to get the output nodes when user does not specify them
    pub fn get_leaf_nodes(&self) -> impl Iterator<Item = &String> {
        // Leaf nodes are those that are not keys in the edges map (i.e., not parents of any node)
        self.nodes
            .keys()
            .filter(move |k| !self.edges.keys().any(|v| v.contains(*k)))
    }

    pub fn get_parents_key_for_node(&self, node_key: &str) -> impl Iterator<Item = &String> {
        // Get the parents for the node_key
        // Parents are those that have the node_key as a child in the edges map
        let node_key = node_key.to_owned();
        self.edges
            .iter()
            .filter_map(move |(key, children)| children.contains(&node_key).then_some(key))
    }
}

impl From<PipelineBuilder> for Pipeline {
    fn from(val: PipelineBuilder) -> Self {
        let output_nodes: HashSet<String> = if val.pipeline.output_nodes.is_empty() {
            // If there are no output nodes, then we need to set the output nodes to the leaf nodes
            val.pipeline.get_leaf_nodes().cloned().collect()
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

#[derive(Serialize, Debug, Clone)]
/// `PipelineJob` struct
/// This struct is used to store the pipeline and the input map
pub struct PipelineJob {
    pub hash: String,
    pub pipeline: Pipeline,
    #[serde(serialize_with = "serialize_hashmap")]
    /// Mapping of outside input to keys to be match with the pipeline `input_map`
    pub input_map: HashMap<String, Input>,
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
            .map(|node_id| match pipeline.get_node(node_id)? {
                Node::Pod(pod) => Ok(find_missing_keys(&input_map, pod.input_stream.keys())),
                Node::Mapper(mapper) => Ok(find_missing_keys(&input_map, mapper.mapping.keys())),
            })
            .collect::<Result<Vec<Vec<String>>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<String>>();

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
            hash: String::new(),
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

/// Helper struct to assist in defining a pipeline in Rust
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
    /// Creates a new `PipelineBuilder` instance.
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

        // Get the node_key to add to the edge
        let node_key = self.get_node_key(&hash);

        // Insert into node hash_map if does not exist, else skip
        self.pipeline.nodes.entry(node.get_hash()).or_insert(node);

        NodeHandle {
            node_key,
            pipeline_builder: self,
        }
    }

    fn add_edge_from_node(&mut self, from: String, node: impl Into<Node>) -> NodeHandle {
        // Check if node exists in the pipeline.nodes
        let node = node.into();
        let hash = node.get_hash();

        // Get the node_key to add to the edge
        let node_key = self.get_node_key(&hash);

        // Insert node into the pipeline.nodes if it does not exist
        self.pipeline.nodes.entry(hash).or_insert(node);

        // Check if the from node exists in the pipeline.edges
        // If it does not exist, then we need to create a new vector for it
        // else we need to push the node_key to the vector
        match self.pipeline.edges.entry(from) {
            Entry::Occupied(mut e) => {
                e.get_mut().push(node_key.clone());
            }
            Entry::Vacant(e) => {
                e.insert(vec![node_key.clone()]);
            }
        }

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

/// Handle to store the `node_key` for the user to add children to it
pub struct NodeHandle<'a> {
    node_key: String,
    pipeline_builder: &'a mut PipelineBuilder,
}

impl NodeHandle<'_> {
    /// Add an node as a child to the current `node_key`
    pub fn add_child(&mut self, node: impl Into<Node>) -> NodeHandle<'_> {
        self.pipeline_builder
            .add_edge_from_node(self.node_key.clone(), node)
    }
}
