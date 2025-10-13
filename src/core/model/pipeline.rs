use std::{
    backtrace::Backtrace,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    result,
};

use crate::{
    core::{crypto::hash_buffer, model::ToYaml},
    uniffi::{
        error::{Kind, OrcaError, Result, selector},
        model::{
            packet::PathSet,
            pipeline::{Kernel, NodeURI, Pipeline, PipelineJob},
        },
    },
};
use itertools::Itertools as _;
use petgraph::{
    Direction::Incoming,
    graph::{self, NodeIndex},
};
use serde::{Deserialize, Serialize, ser::SerializeStruct as _};
use snafu::OptionExt as _;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
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
    /// Validate the pipeline to ensure that, based on user labels:
    /// 1. Each node's `input_spec` is covered by either its parent nodes or the pipeline's `input_spec`
    pub(crate) fn validate(&self) -> Result<()> {
        // For verification we check that each node has it's input_spec covered by either it's parent or input_spec of the pipeline
        // Build a map from input_spec where HashMap<Node_id (Should be label when coming in from new), HashSet<Key covered by input_spec>,
        let mut keys_covered_by_input_spec_lut: HashMap<&String, HashSet<&String>> =
            HashMap::<&String, HashSet<&String>>::new();
        for node_uris in self.input_spec.values() {
            for node_uri in node_uris {
                keys_covered_by_input_spec_lut
                    .entry(&node_uri.node_id)
                    .or_default()
                    .insert(&node_uri.key);
            }
        }

        // Iterate over each node in the graph and verify that its input spec is met
        for node_idx in self.graph.node_indices() {
            self.validate_valid_input_spec(
                node_idx,
                keys_covered_by_input_spec_lut.get(&self.graph[node_idx].hash),
            )?;
        }

        // Build a LUT for all node_hash to idx
        let node_hash_to_idx_lut: HashMap<&String, NodeIndex> = self
            .graph
            .node_indices()
            .map(|idx| (&self.graph[idx].hash, idx))
            .collect();

        // Validate that all output_keys are valid
        self.output_spec.iter().try_for_each(|(_, node_uri)| {
            if !self
                .get_output_spec_for_node(*node_hash_to_idx_lut.get(&node_uri.node_id).context(
                    selector::InvalidOutputSpecNodeNotInGraph {
                        node_name: node_uri.node_id.clone(),
                    },
                )?)
                .contains(&node_uri.key)
            {
                return Err(OrcaError {
                    kind: Kind::InvalidOutputSpecKeyNotInNode {
                        node_name: node_uri.node_id.clone(),
                        key: node_uri.key.clone(),
                        backtrace: Some(Backtrace::capture()),
                    },
                });
            }
            Ok(())
        })?;

        Ok(())
    }

    /// Validates that the input spec for a given node is valid based on its parents and the input spec of the pipeline
    fn validate_valid_input_spec(
        &self,
        node_idx: NodeIndex,
        keys_covered_by_input_spec: Option<&HashSet<&String>>,
    ) -> Result<()> {
        // We need to get the input spec of the current node and build the packet based on the
        // parent nodes output spec + input

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
                    || keys_covered_by_input_spec.is_some_and(|keys| keys.contains(expected_key)))
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
                    .flat_map(|parent_idx| self.get_output_spec_for_node(parent_idx))
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
                    .flat_map(|parent_idx| self.get_output_spec_for_node(parent_idx))
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
    pub(crate) fn compute_hash_for_node_and_parents(
        node_idx: NodeIndex,
        input_spec: &HashMap<String, Vec<NodeURI>>,
        graph: &mut graph::Graph<PipelineNode, ()>,
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
                        Self::compute_hash_for_node_and_parents(parent_idx, input_spec, graph);
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

    fn to_dot_lex(&self) -> String {
        // Get all the nodes and their children in lexicographical order
        let nodes_and_edges = self.graph.node_indices().fold(
            BTreeMap::<&String, BTreeSet<&String>>::new(),
            |mut acc, node_idx| {
                let children = self
                    .graph
                    .neighbors_directed(node_idx, petgraph::Direction::Outgoing)
                    .map(|child_idx| &self.graph[child_idx].hash)
                    .collect::<BTreeSet<&String>>();
                acc.insert(&self.graph[node_idx].hash, children);
                acc
            },
        );

        // Build the dot representation string by
        let mut lines = Vec::new();
        for (node, node_children) in nodes_and_edges {
            if node_children.is_empty() {
                lines.push(format!("  \"{node}\""));
            } else {
                for child in node_children {
                    lines.push(format!("  \"{node}\" -> \"{child}\""));
                }
            }
        }

        // Convert lines into a single string with a proper new line between each entry
        format!("digraph {{\n{}\n}}", lines.join("\n"))
    }

    /// Get a `BTreeMap` of <`kernel_hash`, `BTreeSet`<`node_hashes`>> for all nodes in the graph. Mainly use for serialization
    pub(crate) fn get_kernel_to_node_lut(&self) -> BTreeMap<String, BTreeSet<String>> {
        self.graph.node_indices().fold(
            BTreeMap::<String, BTreeSet<String>>::new(),
            |mut acc, node_idx| {
                acc.entry(self.graph[node_idx].kernel.get_hash().to_owned())
                    .or_default()
                    .insert(self.graph[node_idx].hash.clone());
                acc
            },
        )
    }

    pub(crate) fn get_kernel_lut(&self) -> HashSet<&Kernel> {
        self.graph
            .node_indices()
            .fold(HashSet::<&Kernel>::new(), |mut acc, node_idx| {
                acc.insert(&self.graph[node_idx].kernel);
                acc
            })
    }

    /// Get a `HashMap` of <`node_hash`, `node_label`> for all nodes in the graph if label is not empty. Mainly use for serialization
    pub(crate) fn get_label_lut(&self) -> impl Iterator<Item = (&String, &String)> {
        self.graph.node_indices().filter_map(|node_idx| {
            let label = &self.graph[node_idx].label;
            if label.is_empty() {
                None
            } else {
                Some((&self.graph[node_idx].hash, label))
            }
        })
    }
}

impl Serialize for Pipeline {
    fn serialize<S>(&self, serializer: S) -> result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("Pipeline", 4)?;
        state.serialize_field("kernel_lut", &self.get_kernel_to_node_lut())?;
        state.serialize_field("dot", &self.to_dot_lex())?;

        // Input spec needs to be sorted for consistent serialization
        let input_spec_sorted: BTreeMap<_, Vec<NodeURI>> = self
            .input_spec
            .iter()
            .map(|(k, v)| {
                let mut sorted_v = v.clone();
                sorted_v.sort();
                (k, sorted_v)
            })
            .collect();
        state.serialize_field("input_spec", &input_spec_sorted)?;
        state.serialize_field("output_spec", &self.output_spec)?;
        state.end()
    }
}

impl ToYaml for Pipeline {
    fn process_field(
        field_name: &str,
        field_value: &serde_yaml::Value,
    ) -> Option<(String, serde_yaml::Value)> {
        match field_name {
            "hash" | "annotation" => None, // Skip annotation field
            _ => Some((field_name.to_owned(), field_value.clone())),
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

#[cfg(test)]
mod tests {
    use crate::{
        core::model::ToYaml as _,
        uniffi::{
            error::Result,
            model::{
                Annotation,
                pipeline::{NodeURI, Pipeline},
            },
            operator::MapOperator,
        },
    };
    use indoc::indoc;
    use pretty_assertions::assert_eq;
    use std::collections::HashMap;

    #[test]
    fn to_yaml() -> Result<()> {
        let pipeline = Pipeline::new(
            indoc! {"
            digraph {
                A -> B -> C
            }
        "},
            &HashMap::from([
                (
                    "A".into(),
                    MapOperator::new(HashMap::from([("node_key_1".into(), "node_key_2".into())]))?
                        .into(),
                ),
                (
                    "B".into(),
                    MapOperator::new(HashMap::from([("node_key_2".into(), "node_key_1".into())]))?
                        .into(),
                ),
                (
                    "C".into(),
                    MapOperator::new(HashMap::from([("node_key_1".into(), "node_key_2".into())]))?
                        .into(),
                ),
            ]),
            HashMap::from([(
                "pipeline_key_1".into(),
                vec![NodeURI {
                    node_id: "A".into(),
                    key: "node_key_1".into(),
                }],
            )]),
            HashMap::new(),
            Some(Annotation {
                name: "test".into(),
                version: "0.1".into(),
                description: "Test pipeline".into(),
            }),
        )?;

        assert_eq!(
            pipeline.to_yaml()?,
            indoc! {r#"
            class: pipeline
            kernel_lut:
              2980eb39e3702442cc31656d6ec3995f91680ab042a27160a00ffe33b91419af:
              - 4b498582ed57ca6a10809d7480bd3f159542ad139e402698e3f525fc6b0d4dea
              c8f036079b69beee914434c1e01be638972ce05cd2e640fc1e9be7bf3d9e76be:
              - 368c7a517f3fbdd7cab10c90ebc44e44765fc33f66b4f4f5151a6bee322d8217
              - 6c84111298d0cfe811dff3f10ab444795c0a9d60609ba1b9391c45e642a69afa
            dot: |-
              digraph {
                "368c7a517f3fbdd7cab10c90ebc44e44765fc33f66b4f4f5151a6bee322d8217"
                "4b498582ed57ca6a10809d7480bd3f159542ad139e402698e3f525fc6b0d4dea" -> "368c7a517f3fbdd7cab10c90ebc44e44765fc33f66b4f4f5151a6bee322d8217"
                "6c84111298d0cfe811dff3f10ab444795c0a9d60609ba1b9391c45e642a69afa" -> "4b498582ed57ca6a10809d7480bd3f159542ad139e402698e3f525fc6b0d4dea"
              }
            input_spec:
              pipeline_key_1:
              - node_id: 6c84111298d0cfe811dff3f10ab444795c0a9d60609ba1b9391c45e642a69afa
                key: node_key_1
            output_spec: {}
            "#},
        );

        Ok(())
    }
}
