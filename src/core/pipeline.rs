use serde::{Deserialize, Serialize};
use std::{backtrace::Backtrace, collections::HashMap};

use crate::uniffi::{
    error::{Kind, OrcaError, Result},
    model::Pod,
};

use crate::core::model::serialize_hashmap;

use super::{crypto::hash_buffer, model::to_yaml};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum Node {
    Pod(Box<PodNode>),
    Mapper(MapperNode),
}

impl From<Pod> for Node {
    fn from(pod: Pod) -> Self {
        Self::Pod(Box::new(PodNode {
            pod,
            children: Vec::new(),
        }))
    }
}

pub trait NodeFunctions {
    fn get_hash(&self) -> &String;

    fn get_children(&self) -> &Vec<Node>;

    fn notify_children(&self) {}

    fn process(&self) {}

    fn get_input_stream_keys(&self) -> impl Iterator<Item = &String>;

    fn add_child(&mut self, child: impl Into<Node>) -> &Node;

    fn get_last_added_child(&self) -> Result<&Node>;
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

        self.get_last_added_child().unwrap()
    }

    fn get_last_added_child(&self) -> Result<&Node> {
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

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct MapperNode {
    mapper: Mapper,
    children: Vec<Node>,
}

impl MapperNode {
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
        self.get_last_added_child().unwrap()
    }

    fn get_last_added_child(&self) -> Result<&Node> {
        self.children.last().ok_or(OrcaError {
            kind: Kind::FailToGetLastAddedNode {
                backtrace: Some(Backtrace::capture()),
            },
        })
    }

    fn notify_children(&self) {
        todo!()
    }

    fn process(&self) {
        todo!()
    }

    fn get_hash(&self) -> &String {
        &self.mapper.hash
    }

    fn get_children(&self) -> &Vec<Node> {
        &self.children
    }
}

// struct EdgeInfo<T: NodeFunctions> {
//     from: T,
//     to: BTreeMap<Node, ()>,
// }

#[derive(Serialize, Deserialize, Debug)]
pub struct Pipeline<T: NodeFunctions> {
    hash: String,
    root_nodes: Vec<T>,
}

impl<T: NodeFunctions> Pipeline<T> {
    // pub fn new(root_nodes: Vec<T>) -> Result<Self> {
    //     // Recursively hash the parent and children with sort
    //     let edges = Vec::new();
    // }

    // fn extract_edges(node: T, edge_buffer: &mut Vec<EdgeInfo<T>>) {
    //     edge_buffer.push(EdgeInfo {
    //         from: node,
    //         to: node
    //             .get_children()
    //             .iter()
    //             .map(|child| {
    //                 let mut child_edges = Vec::new();
    //                 Self::extract_edges(child, &mut child_edges);
    //                 child_edges
    //             })
    //             .collect(),
    //     });
    // }
}

// #[derive(Serialize, Deserialize, Debug)]
// struct PipelineJob {
//     pub pipeline: Pipeline,
//     #[serde(serialize_with = "serialize_hashmap")]
//     pub input_map: HashMap<String, Input>,
// }

// impl PipelineJob {
//     fn new(pipeline: Pipeline, input_map: HashMap<String, Input>) -> Result<Self> {
//         // Check if input_stream has all the correct mapping
//         if let Some(missing_key) = pipeline
//             .root_nodes
//             .iter()
//             .flat_map(|node| node.get_input_stream_keys())
//             .find(|key| !input_map.contains_key(*key))
//         {
//             return Err(OrcaError {
//                 kind: Kind::MissingStreamKey {
//                     input_map,
//                     key: missing_key.clone(),
//                     backtrace: Some(Backtrace::capture()),
//                 },
//             });
//         }

//         Ok(Self {
//             pipeline,
//             input_map,
//         })
//     }
// }

// struct PipelineResult {
//     result: Vec<PodResult>,
// }

// struct PipelineRun {
//     pipeline: Pipeline,
//     orchestrator: dyn Orchestrator,
// }

// struct PipelineRunInfo {
//     status: Status,
//     // Fill the rest out later
// }

// struct PipelineRunner {
//     orchestrator: Box<dyn Orchestrator>,
//     active_pipelines: Vec<PipelineJob>,
// }
