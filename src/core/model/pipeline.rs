use std::{
    backtrace::Backtrace,
    collections::{HashMap, HashSet},
};

use crate::uniffi::{
    error::{Kind, OrcaError, Result},
    model::{
        packet::PathSet,
        pipeline::{Kernel, Pipeline, PipelineJob},
    },
};
use itertools::Itertools as _;
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

    /// Return a vec of `node_names` that takes in inputs based on the `input_spec`
    pub(crate) fn get_input_nodes(&self) -> HashSet<&String> {
        let mut input_nodes = HashSet::new();

        self.input_spec.iter().for_each(|(_, node_uris)| {
            for node_uri in node_uris {
                input_nodes.insert(&node_uri.node_id);
            }
        });

        input_nodes
    }
}

impl PipelineJob {
    /// Helpful function to get the input packet for input nodes of the pipeline based on the `pipeline_job` an`pipeline_spec`ec
    /// # Errors
    /// Will return `Err` if there is an issue getting the input packet per node.
    /// # Returns
    /// A `HashMap` where the key is the node name and the value is a vector of `HashMap<String, PathSet>` representing the input packets for that node.
    #[expect(clippy::excessive_nesting, reason = "Nesting manageable")]
    pub fn get_input_packet_per_node(
        &self,
    ) -> Result<HashMap<String, Vec<HashMap<String, PathSet>>>> {
        // For each node in the input specification, we will iterate over its mapping and
        let mut node_input_spec = HashMap::new();
        for (input_key, node_uris) in &self.pipeline.input_spec {
            for node_uri in node_uris {
                let input_path_sets = self.input_packet.get(input_key).ok_or(OrcaError {
                    kind: Kind::KeyMissing {
                        key: input_key.clone(),
                        backtrace: Some(Backtrace::capture()),
                    },
                })?;
                // There shouldn't be a duplicate key in the input packet
                let node_input_path_sets_ref = node_input_spec
                    .entry(&node_uri.node_id)
                    .or_insert_with(HashMap::new);

                // Check if the node_uri.key already exists, if it does this is an error as there can't be two input_packet that map to the same key
                if node_input_path_sets_ref.contains_key(&node_uri.key) {
                    todo!()
                } else {
                    // Insert all the input_path_sets that map to this specific key for the node
                    node_input_path_sets_ref.insert(&node_uri.key, input_path_sets);
                }
            }
        }

        // For each node, compute the cartesian product of the path_sets for each unique combination of keys
        let node_input_packets = node_input_spec
            .into_iter()
            .map(|(node_id, input_node_keys)| {
                // We need to pull them out at the same time to ensure the key order is preserve to match the cartesian product
                let (keys, values): (Vec<_>, Vec<_>) = input_node_keys.into_iter().unzip();

                // Covert each combo into a packet
                let packets = values
                    .into_iter()
                    .multi_cartesian_product()
                    .map(|combo| {
                        keys.iter()
                            .copied()
                            .zip(combo)
                            .map(|(key, pathset)| (key.to_owned(), pathset.to_owned()))
                            .collect::<HashMap<_, _>>()
                    })
                    .collect::<Vec<HashMap<String, PathSet>>>();

                (node_id.to_owned(), packets)
            })
            .collect::<HashMap<_, _>>();

        Ok(node_input_packets)
    }
}
