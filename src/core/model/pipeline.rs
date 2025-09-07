use std::{
    backtrace::Backtrace,
    collections::{HashMap, HashSet},
};

use crate::{
    core::{crypto::hash_buffer, util::get},
    uniffi::{
        error::{Kind, OrcaError, Result},
        model::{
            packet::PathSet,
            pipeline::{Kernel, NodeURI, Pipeline, PipelineJob},
        },
    },
};
use itertools::Itertools as _;
use petgraph::{
    Direction::Incoming,
    graph::{DiGraph, NodeIndex},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct PipelineNode {
    // Hash that represent the node
    pub hash: String,
    /// Kernel associated with the node
    pub kernel: Kernel,
    /// User provided label for the node
    pub label: String,
    /// This is meant for internal use only to track the node index in the graph
    pub node_idx: NodeIndex,
}

impl Pipeline {
    pub(crate) fn validate(&self) -> Result<()> {
        // For verification we check that each node has it's input_spec covered by either it's parent or input_spec
        // Build a map from input_spec where HashMap<Node_id (Should be label when coming in from new), Vec<InputKeys>,
        let mut input_nodes_key_lut = HashMap::<&String, HashSet<&String>>::new();
        for (input_key, node_uris) in &self.input_spec {
            for node_uri in node_uris {
                input_nodes_key_lut
                    .entry(&node_uri.node_id)
                    .or_default()
                    .insert(input_key);
            }
        }

        // Iterate over each node in the graph and verify that its input spec is met
        for node_idx in self.graph.node_indices() {
            self.validate_valid_input_spec(
                node_idx,
                get(&input_nodes_key_lut, &self.graph[node_idx].label)?,
            )?;
        }

        Ok(())
    }

    fn validate_valid_input_spec(
        &self,
        node_idx: NodeIndex,
        input_keys_for_node: &HashSet<&String>,
    ) -> Result<()> {
        // We need to get the input spec of the current node and build the packet based on the
        // parent nodes to verify that the input_spec if met

        // Get the parent nodes input specs and combine them into
        let incoming_packet_keys = self
            .get_parent_node_indices(node_idx)
            .flat_map(|parent_idx| self.get_output_spec_for_node(parent_idx))
            .collect::<HashSet<&String>>();

        // Get this node input_spec
        let missing_keys: HashSet<&String> = self
            .get_input_spec_for_node(node_idx)
            .into_iter()
            .filter(|expected_key| {
                !(incoming_packet_keys.contains(expected_key)
                    || input_keys_for_node.contains(expected_key))
            })
            .collect();

        // Verify that there are no missing keys, otherwise return error
        if !missing_keys.is_empty() {
            return Err(OrcaError {
                kind: Kind::PipelineValidationErrorMissingKeys {
                    node_name: self.graph[node_idx].label.clone(),
                    missing_keys: missing_keys.into_iter().cloned().collect(),
                    backtrace: Some(Backtrace::capture()),
                },
            });
        }
        Ok(())
    }

    fn get_input_spec_for_node(&self, node_idx: NodeIndex) -> HashSet<&String> {
        match &self.graph[node_idx].kernel {
            Kernel::Pod { pod } => pod.input_spec.keys().collect(),
            Kernel::JoinOperator => {
                // JoinOperator input_spec is derived from its parents
                self.get_parent_node_indices(node_idx)
                    .flat_map(|parent_idx| self.get_input_spec_for_node(parent_idx))
                    .collect()
            }
            Kernel::MapOperator { mapper } => mapper.map.keys().collect(),
        }
    }

    fn get_output_spec_for_node(&self, node_idx: NodeIndex) -> HashSet<&String> {
        match &self.graph[node_idx].kernel {
            Kernel::Pod { pod } => pod.output_spec.keys().collect(),
            Kernel::JoinOperator => {
                // JoinOperator output_spec is derived from its parents
                self.get_parent_node_indices(node_idx)
                    .flat_map(|parent_idx| self.get_input_spec_for_node(parent_idx))
                    .collect()
            }
            Kernel::MapOperator { mapper } => mapper.map.values().collect(),
        }
    }

    /// Function to get the parents of a node
    pub(crate) fn get_node_parents(
        &self,
        node: &PipelineNode,
    ) -> Result<impl Iterator<Item = &PipelineNode>> {
        // Find the NodeIndex for the given node_key
        let node_idx = self
            .graph
            .node_indices()
            .find(|&idx| self.graph[idx] == *node)
            .ok_or(OrcaError {
                kind: Kind::KeyMissing {
                    key: node.label.clone(),
                    backtrace: Some(Backtrace::capture()),
                },
            })?;

        Ok(self
            .get_parent_node_indices(node_idx)
            .map(|parent_idx| &self.graph[parent_idx]))
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

    fn get_parent_node_indices(&self, node_idx: NodeIndex) -> impl Iterator<Item = NodeIndex> {
        self.graph.neighbors_directed(node_idx, Incoming)
    }

    /// Find the leaf nodes in the graph (nodes with no outgoing edges)
    /// # Returns
    /// A vector of `NodeIndex` representing the leaf nodes in the graph
    pub fn find_leaf_nodes(&self) -> Vec<NodeIndex> {
        self.graph
            .node_indices()
            .filter(|&idx| {
                self.graph
                    .neighbors_directed(idx, petgraph::Direction::Outgoing)
                    .next()
                    .is_none()
            })
            .collect()
    }

    /// Compute the hash for each node in the graph which is defined as the hash of its kernel + the hashes of its parents
    pub(crate) fn compute_hash_for_node(
        node_idx: NodeIndex,
        input_spec: &HashMap<String, Vec<NodeURI>>,
        graph: &mut DiGraph<PipelineNode, ()>,
    ) {
        // Collect parent indices first to avoid borrowing issues
        let parent_indices: Vec<NodeIndex> = graph.neighbors_directed(node_idx, Incoming).collect();

        // Sort the parent hashes to ensure consistent ordering
        let mut parent_hashes: Vec<String> = if parent_indices.is_empty() {
            // This is parent node, thus we will need to use the input_spec to generate a unique hash for the node
            // Find all the input keys that map to this node
            let input_keys = input_spec.iter().filter_map(|(input_key, node_uris)| {
                node_uris.iter().find_map(|node_uri| {
                    (node_uri.node_id == graph[node_idx].label).then(|| input_key.clone())
                })
            });

            input_keys.collect()
        } else {
            parent_indices
                .into_iter()
                .map(|parent_idx| {
                    // Check if hash has been computed for this node, if not trigger computation
                    if graph[parent_idx].hash.is_empty() {
                        // Recursive call to compute the parent's hash
                        Self::compute_hash_for_node(parent_idx, input_spec, graph);
                    }
                    graph[parent_idx].hash.clone()
                })
                .collect()
        };

        parent_hashes.sort();

        // Combine the node's kernel hash + the parent_hashes by concatenation only if there are parents hashes, else it is just the kernel hash
        if parent_hashes.is_empty() {
        } else {
            let hash_for_node = format!(
                "{}{}",
                &graph[node_idx].kernel.get_hash(),
                parent_hashes.into_iter().join("")
            );
            graph[node_idx].hash = hash_buffer(hash_for_node.as_bytes());
        }
    }
}

impl PipelineJob {
    /// Helpful function to get the input packet for input nodes of the pipeline based on the `pipeline_job` an`pipeline_spec`ec
    /// # Errors
    /// Will return `Err` if there is an issue getting the input packet per node.
    /// # Returns
    /// A `HashMap` where the key is the node name and the value is a vector of `HashMap<String, PathSet>` representing the input packets for that node.
    pub fn get_input_packet_per_node(
        &self,
    ) -> Result<HashMap<String, Vec<HashMap<String, PathSet>>>> {
        // For each node in the input specification, we will iterate over its mapping
        // nodes_input_spec contains <node_id, HashMap<key, PathSet>>
        let mut nodes_input_spec = HashMap::new();
        for (input_key, node_uris) in &self.pipeline.input_spec {
            for node_uri in node_uris {
                let input_path_sets = self.input_packet.get(input_key).ok_or(OrcaError {
                    kind: Kind::KeyMissing {
                        key: input_key.clone(),
                        backtrace: Some(Backtrace::capture()),
                    },
                })?;
                // There shouldn't be a duplicate key in the input packet as this will be handle by pipeline verify
                let input_spec = nodes_input_spec
                    .entry(&node_uri.node_id)
                    .or_insert_with(HashMap::new);
                input_spec.insert(&node_uri.key, input_path_sets);
            }
        }

        // For each node, compute the cartesian product of the path_sets for each unique combination of keys
        let node_input_packets = nodes_input_spec
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
