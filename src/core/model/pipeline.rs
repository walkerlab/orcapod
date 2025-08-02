use std::collections::HashSet;

use crate::uniffi::model::pipeline::{Kernel, Pipeline};
use petgraph::Direction::Incoming;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct PipelineNode {
    pub name: String,
    pub kernel: Kernel,
}

impl Pipeline {
    /// Function to get the parents of a node
    pub(crate) fn get_node_parents(
        &self,
        node: &PipelineNode,
    ) -> impl Iterator<Item = &PipelineNode> {
        // Find the NodeIndex for the given node_key
        let node_index = self
            .graph
            .node_indices()
            .find(|&idx| self.graph[idx] == *node);
        node_index.into_iter().flat_map(move |idx| {
            self.graph
                .neighbors_directed(idx, Incoming)
                .map(move |parent_idx| &self.graph[parent_idx])
        })
    }

    /// Return a vec of `node_names` that takes in inputs based on the `input_spec`ec
    pub(crate) fn get_input_nodes(&self) -> HashSet<&String> {
        let mut input_nodes = HashSet::new();

        self.input_spec.iter().for_each(|(_, node_uris)| {
            for node_uri in node_uris {
                input_nodes.insert(&node_uri.node_name);
            }
        });

        input_nodes
    }
}
