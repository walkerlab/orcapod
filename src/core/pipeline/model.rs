use serde::{Deserialize, Serialize};
use snafu::OptionExt as _;
use std::{backtrace::Backtrace, collections::HashMap};

use crate::uniffi::{
    error::{Kind, OrcaError, Result, selector},
    model::{Input, Pod},
};

use crate::core::model::serialize_hashmap;

#[derive(Serialize, Deserialize, Debug)]
enum Node {
    PodNode(Box<PodNode>),
    Mapper(Mapper),
}

impl Node {
    fn get_input_stream_keys(&self) -> Vec<&String> {
        match self {
            Self::PodNode(pod_node) => pod_node.get_input_stream_keys(),
            Self::Mapper(mapper) => mapper.get_input_stream_keys(),
        }
    }
}

trait NodeFunctions {
    fn notify_children(&self) {}

    fn process(&self) {}

    fn get_input_stream_keys(&self) -> Vec<&String>;
}

#[derive(Serialize, Deserialize, Debug)]
struct PodNode {
    pod: Pod,
    children: Vec<Node>,
}

impl NodeFunctions for PodNode {
    fn get_input_stream_keys(&self) -> Vec<&String> {
        self.pod.input_stream.keys().collect()
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Mapper {
    mapping: HashMap<String, String>,
    children: Vec<Node>,
}

impl NodeFunctions for Mapper {
    fn get_input_stream_keys(&self) -> Vec<&String> {
        self.mapping.keys().collect()
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Pipeline {
    root_nodes: Vec<Node>,
}

impl Pipeline {}

#[derive(Serialize, Deserialize, Debug)]
struct PipelineJob {
    pub pipeline: Pipeline,
    #[serde(serialize_with = "serialize_hashmap")]
    pub input_map: HashMap<String, Input>,
}

impl PipelineJob {
    fn new(pipeline: Pipeline, input_map: HashMap<String, Input>) -> Result<Self> {
        // Check if input_stream has all the correct mapping
        if let Some(missing_key) = pipeline
            .root_nodes
            .iter()
            .flat_map(|node| node.get_input_stream_keys())
            .find(|key| !input_map.contains_key(*key))
        {
            return Err(OrcaError {
                kind: Kind::MissingStreamKey {
                    input_map,
                    key: missing_key.clone(),
                    backtrace: Some(Backtrace::capture()),
                },
            });
        }

        Ok(Self {
            pipeline,
            input_map,
        })
    }
}
