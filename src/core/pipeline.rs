use serde::{Deserialize, Serialize};
use std::{backtrace::Backtrace, collections::HashMap};
use tokio::{sync::RwLock, task::JoinHandle};

use crate::uniffi::{
    error::{Kind, OrcaError, Result},
    model::{Annotation, Input, Pod},
    orchestrator::Orchestrator,
};

use crate::core::model::serialize_hashmap;

use super::{crypto::hash_buffer, model::to_yaml};

/// Enum for storing different types of nodes in the pipeline
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum Node {
    /// Pod node
    Pod(Box<PodNode>),
    /// Mapper node
    Mapper(MapperNode),
}

impl Node {
    pub fn get_children(&self) -> &Vec<Self> {
        match self {
            Self::Pod(pod_node) => pod_node.get_children(),
            Self::Mapper(mapper_node) => mapper_node.get_children(),
        }
    }

    pub async fn process(&self, join_handles: &mut Vec<JoinHandle<()>>) {
        match self {
            Self::Pod(pod_node) => pod_node.process(join_handles).await,
            Self::Mapper(mapper_node) => mapper_node.process(join_handles).await,
        }
    }
}

impl From<Pod> for Node {
    fn from(pod: Pod) -> Self {
        Self::Pod(Box::new(PodNode::new(pod)))
    }
}

impl From<Mapper> for Node {
    fn from(mapper: Mapper) -> Self {
        Self::Mapper(MapperNode::new(mapper))
    }
}

/// Trait for node functions
/// This trait defines the functions that all nodes must implement
pub trait NodeFunctions {
    /// Get the hash of the node
    fn get_hash(&self) -> &String;

    /// Get the children of the node
    fn get_children(&self) -> &Vec<Node>;

    async fn process(&self, pipeline_run: &impl PipelineRun);

    fn get_input_stream_keys(&self) -> impl Iterator<Item = &String>;

    fn add_child(&mut self, child: impl Into<Node>) -> &Node;

    /// # Errors
    /// Will error if it fails to get the last child in the vector
    fn get_last_child(&self) -> Result<&Node>;
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct PodNode {
    pod: Pod,
    children: Vec<Node>,
}

impl PodNode {
    pub const fn new(pod: Pod) -> Self {
        Self {
            pod,
            children: Vec::new(),
        }
    }
}

impl NodeFunctions for PodNode {
    fn get_input_stream_keys(&self) -> impl Iterator<Item = &String> {
        self.pod.input_stream.keys()
    }

    #[expect(clippy::unwrap_used, reason = "This should never fail")]
    fn add_child(&mut self, child: impl Into<Node>) -> &Node {
        self.children.push(child.into());

        self.get_last_child().unwrap()
    }

    fn get_last_child(&self) -> Result<&Node> {
        self.children.last().ok_or(OrcaError {
            kind: Kind::FailToGetLastAddedNode {
                backtrace: Some(Backtrace::capture()),
            },
        })
    }

    fn get_hash(&self) -> &String {
        &self.pod.hash
    }

    fn get_children(&self) -> &Vec<Node> {
        &self.children
    }

    async fn process(&self, pipeline_run: &impl PipelineRun) {
        println!("Processing pod node: {:?}", self.pod.hash);

        // Spin up a new thread for each of the children
        pipeline_run.get_join_handles().blocking_write().extend(
            self.children
                .iter()
                .map(|child| tokio::spawn(child.process(join_handles)))
                .collect::<Vec<_>>(),
        );
    }
}

/// Mapper
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct Mapper {
    hash: String,
    #[serde(serialize_with = "serialize_hashmap")]
    mapping: HashMap<String, String>,
}

impl Mapper {
    /// New function for mapping that computes the hash for
    /// # Errors
    /// Will error if it fails to convert to yaml
    pub fn new(mapping: HashMap<String, String>) -> Result<Self> {
        let no_hash = Self {
            hash: String::new(),
            mapping,
        };

        Ok(Self {
            hash: hash_buffer(to_yaml(&no_hash)?),
            ..no_hash
        })
    }
}

/// Node design for renaming streams and inputs
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct MapperNode {
    mapper: Mapper,
    children: Vec<Node>,
}

impl MapperNode {
    /// New function for mapper node
    pub const fn new(mapper: Mapper) -> Self {
        Self {
            mapper,
            children: Vec::new(),
        }
    }
}

impl NodeFunctions for MapperNode {
    fn get_input_stream_keys(&self) -> impl Iterator<Item = &String> {
        self.mapper.mapping.keys()
    }

    #[expect(clippy::unwrap_used, reason = "This should never fail")]
    fn add_child(&mut self, child: impl Into<Node>) -> &Node {
        self.children.push(child.into());
        self.get_last_child().unwrap()
    }

    fn get_last_child(&self) -> Result<&Node> {
        self.children.last().ok_or(OrcaError {
            kind: Kind::FailToGetLastAddedNode {
                backtrace: Some(Backtrace::capture()),
            },
        })
    }

    fn get_hash(&self) -> &String {
        &self.mapper.hash
    }

    fn get_children(&self) -> &Vec<Node> {
        &self.children
    }
}

#[derive(PartialEq)]
struct EdgeInfo<'a> {
    from: &'a Node,
    to: &'a Vec<Node>,
}

/// Pipeline struct
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Pipeline {
    hash: String,
    #[serde(skip)]
    annotation: Option<Annotation>,
    root_nodes: Vec<Node>,
}

impl Pipeline {
    /// New function for pipeline
    /// # Errors
    /// Will error if it fails to convert to yaml
    pub fn new(root_nodes: Vec<Node>, annotation: Option<Annotation>) -> Result<Self> {
        let no_hash = Self {
            hash: String::new(),
            annotation,
            root_nodes,
        };

        Ok(Self {
            hash: hash_buffer(to_yaml(&no_hash)?),
            ..no_hash
        })
    }

    pub fn get_edges_vec(&self) -> Vec<EdgeInfo> {
        let mut edge_buffer = Vec::new();

        // Iterate over the root nodes and extract edges
        self.root_nodes.iter().for_each(|node| {
            Self::extract_edges(node, &mut edge_buffer);
        });

        edge_buffer
    }

    pub fn extract_edges<'a>(node: &'a Node, edge_buffer: &mut Vec<EdgeInfo<'a>>) {
        // Add the current node to the edge buffer
        edge_buffer.push(EdgeInfo {
            from: node,
            to: node.get_children(),
        });

        // Recursively add the children to the edge buffer
        node.get_children().iter().for_each(|child| {
            Self::extract_edges(child, edge_buffer);
        });
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PipelineJob {
    pub pipeline: Pipeline,
    #[serde(serialize_with = "serialize_hashmap")]
    pub input_map: HashMap<String, Input>,
    pub annotation: Option<Annotation>,
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
            .root_nodes
            .iter()
            .flat_map(|node| match node {
                Node::Pod(pod_node) => {
                    find_missing_keys(&input_map, pod_node.pod.input_stream.keys())
                }
                Node::Mapper(mapper_node) => {
                    find_missing_keys(&input_map, mapper_node.mapper.mapping.keys())
                }
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
struct DockerPipelineRun {
    pipeline: Pipeline,
    join_handles: RwLock<Vec<JoinHandle<()>>>,
}

impl PipelineRun for DockerPipelineRun {
    fn get_join_handles(&self) -> &RwLock<Vec<JoinHandle<()>>> {
        &self.join_handles
    }
}

// struct PipelineRunInfo {
//     status: Status,
//     // Fill the rest out later
// }

struct DockerRunner {
    orchestrator: Box<dyn Orchestrator>,
    active_pipelines: RwLock<Vec<DockerPipelineRun>>,
}
