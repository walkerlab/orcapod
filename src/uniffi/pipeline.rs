use crate::{
    core::{crypto::hash_buffer, model::to_yaml, util::get},
    uniffi::{
        error::{Kind, OrcaError, Result},
        model::{Annotation, PathSet, Pod, URI},
    },
};
use derive_more::Display;
use dot_parser::{ast, canonical};
use petgraph::Direction::{Incoming, Outgoing};
use petgraph::{graph::DiGraph, prelude::NodeIndex};
use serde::Serialize;
use std::{
    backtrace::Backtrace,
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    string::String,
    sync::Arc,
};

/// Pipeline Components
/// Mapper
#[derive(uniffi::Object, Display, Serialize, Debug, PartialEq, Eq, Clone)]
#[display("{self:#?}")]
#[uniffi::export(Display)]
pub struct Mapper {
    /// Hash of the Mapper
    pub hash: String,
    /**
    Mapping of `input_stream_keys` to `output_stream_keys` of the mapper
    */
    pub mapping: HashMap<String, String>,
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

#[derive(uniffi::Enum, Debug, Clone)]
/// Enum to store different types of nodes explicitly
pub enum Kernel {
    /// Pod node
    Pod(Arc<Pod>),
    /// Mapper node
    Mapper(Arc<Mapper>),
    /// Joiner node
    Joiner,
}

impl Kernel {
    /// Get the hash of the node
    pub fn get_hash(&self) -> String {
        match self {
            Self::Pod(pod) => pod.hash.clone(),
            Self::Mapper(mapper) => mapper.hash.clone(),
            Self::Joiner => hash_buffer(b"Joiner"),
        }
    }

    fn get_input_keys(&self) -> Vec<&String> {
        match self {
            Self::Pod(pod) => pod.input_spec.keys().collect(),
            Self::Mapper(mapper) => mapper.mapping.keys().collect(),
            Self::Joiner => Vec::new(), // Joiner does not have input keys
        }
    }

    /// Returns the output keys for the kernel, except for joiner which returns None
    fn get_output_keys(&self) -> Vec<&String> {
        match self {
            Self::Pod(pod) => pod.output_spec.keys().collect(),
            Self::Mapper(mapper) => mapper.mapping.values().collect(),
            Self::Joiner => Vec::new(), // Joiner does not have output keys
        }
    }
}

impl Hash for Kernel {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Pod(pod) => pod.hash.hash(state),
            Self::Mapper(mapper) => mapper.hash.hash(state),
            Self::Joiner => "Joiner".hash(state),
        }
    }
}

impl PartialEq for Kernel {
    fn eq(&self, other: &Self) -> bool {
        self.get_hash() == other.get_hash()
    }
}

impl Eq for Kernel {}

impl From<Pod> for Kernel {
    fn from(pod: Pod) -> Self {
        Self::Pod(Arc::new(pod))
    }
}
impl From<Mapper> for Kernel {
    fn from(mapper: Mapper) -> Self {
        Self::Mapper(Arc::new(mapper))
    }
}

#[derive(uniffi::Object, Display, Debug, Default, Clone, PartialEq, Eq)]
#[display("{self:#?}")]
#[uniffi::export(Display)]
/// Struct to represent a node in the pipeline graph
pub struct Node {
    /// This is id for now till hashing feature get merged
    pub id: String,
    /// Hash of the kernel to use in `kernel_lut`
    pub kernel_hash: String,
}

impl Node {
    /// Creates a new `Node` instance and computes its hash based on the kernel hash and parent hashes.
    pub fn new(kernel_hash: &str, parent_hashes: Vec<&str>) -> Self {
        Self {
            id: Self::compute_hash(kernel_hash, parent_hashes),
            kernel_hash: kernel_hash.to_owned(),
        }
    }

    fn compute_hash(kernel_hash: &str, parent_hashes: Vec<&str>) -> String {
        // Sort the parent hashes to ensure consistent ordering
        let mut sorted_hashes = parent_hashes;
        sorted_hashes.sort_unstable();

        // Combine all parent hashes with the kernel hash
        let mut buffer = kernel_hash.to_owned();
        for hash in sorted_hashes {
            buffer.push_str(hash);
        }

        hash_buffer(buffer.as_bytes())
    }
}

/// Pipeline struct
#[derive(uniffi::Object, Debug, Default, Clone, Display)]
#[display("{self:#?}")]
#[uniffi::export(Display)]
pub struct Pipeline {
    /// Annotation for the pipeline
    pub annotation: Option<Annotation>,
    /// Strings are the hash of the kernel, and the value is the kernel itself.
    /// Mainly used to prevent duplicate storage of kernels share by multiple nodes.
    /// NOTE: String is currently whatever, in the future it should be a hash
    pub kernel_lut: HashMap<String, Kernel>,
    /// Labels provided by the user for each node where the key is the node hash and the value the actual label
    pub graph: DiGraph<Node, ()>,
}

impl Pipeline {
    /// Creates a new `Pipeline` instance.
    /// # Errors
    /// Will error out if the pipeline is not valid
    pub const fn new(
        annotation: Option<Annotation>,

        kernel_lut: HashMap<String, Kernel>,
        graph: DiGraph<Node, ()>,
    ) -> Self {
        Self {
            annotation,
            kernel_lut,
            graph,
        }
    }

    /// Creates a new `Pipeline` instance from a DOT string and kernel lookup table.
    /// The `kernel_lut` is the nodes while the dot is the edges.
    ///
    /// # Errors
    /// Will error out if the DOT string is invalid or if the graph name cannot be parsed
    pub fn from_dot(
        kernel_to_node_name: &HashMap<Kernel, Vec<String>>,
        dot: &str,
        annotation: Option<Annotation>,
    ) -> Result<Self> {
        let parsed_graph =
            canonical::Graph::from(ast::Graph::try_from(dot).map_err(|err| OrcaError {
                kind: Kind::FailedToParseDot {
                    dot: dot.to_owned(),
                    reason: err.to_string(),
                    backtrace: Some(Backtrace::capture()),
                },
            })?);

        let mut graph = DiGraph::new();

        // Create the kernel_lut and node_idx_lut
        let mut kernel_lut = HashMap::new();
        let mut node_idx_lut: HashMap<String, NodeIndex> = HashMap::new();

        for (kernel, node_names) in kernel_to_node_name {
            // Insert into kernel_lut
            kernel_lut
                .entry(kernel.get_hash())
                .or_insert_with(|| kernel.clone());

            // Create the node, insert into graph and store the idx
            for node_name in node_names {
                let node = Node {
                    id: (*node_name).clone(),
                    kernel_hash: kernel.get_hash(),
                };
                let node_idx = graph.add_node(node);
                node_idx_lut.insert(node_name.to_owned(), node_idx);
            }
        }

        // Build the graph from the dot
        parsed_graph.edges.set.iter().try_for_each(|edge| {
            graph.add_edge(
                *get(&node_idx_lut, &edge.from)?,
                *get(&node_idx_lut, &edge.to)?,
                (),
            );
            Ok::<(), OrcaError>(())
        })?;

        Ok(Self {
            annotation,
            kernel_lut,
            graph,
        })
    }

    /// Returns the input specification for the pipeline, where the specification is a list of unique
    /// keys that are required as input to the pipeline.
    ///
    /// # Errors
    /// Will error out if it fails to get the kernel from the kernel lookup table
    pub fn get_input_spec(&self) -> Result<Vec<&String>> {
        // Input spec is currently all the root nodes input_spec combined for now
        // TODO: This will be replaced when input_nodes is implemented

        Ok(self
            .get_root_nodes()
            .map(|node| Ok(get(&self.kernel_lut, &node.kernel_hash)?.get_input_keys()))
            .collect::<Result<HashSet<_>>>()?
            .into_iter()
            .flatten()
            .collect())
    }

    /// Returns the output specification for the pipeline
    /// This is currently a combination of all the leaf nodes' output specifications.
    ///
    /// # Errors
    /// Will error out if it fails to get the kernel from the kernel lookup table
    pub fn get_output_spec(&self) -> Result<Vec<&String>> {
        // Output spec is currently the all the leaf nodes output_spec combined for now
        // TODO Later this will be replaced when output_nodes is implemented

        Ok(self
            .get_leaf_nodes()
            .map(|node| Ok(get(&self.kernel_lut, &node.kernel_hash)?.get_output_keys()))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect())
    }

    /// # Errors
    /// Error out if the `kernel_key` is not found in the `kernel_lut`
    #[expect(clippy::string_slice, reason = "Should never fail as we are in")]
    pub fn get_kernel(&self, kernel_key: &str) -> Result<&Kernel> {
        let char_to_cut_at = '_';

        let key = kernel_key
            .rfind(char_to_cut_at)
            .map_or(kernel_key, |index| &kernel_key[..index]);
        get(&self.kernel_lut, &key.to_owned())
    }

    /// Function to get the root nodes of the pipeline
    pub fn get_root_nodes(&self) -> impl Iterator<Item = &Node> {
        self.graph
            .node_indices()
            .filter(|&node_index| self.graph.neighbors_directed(node_index, Incoming).count() == 0)
            .map(|node_index| &self.graph[node_index])
    }

    /// Looks through the graph and find nodes that don't have any parents
    pub fn get_root_nodes_idx(&self) -> impl Iterator<Item = NodeIndex> {
        self.graph
            .node_indices()
            .filter(|&node_index| self.graph.neighbors_directed(node_index, Incoming).count() == 0)
    }

    /// Function to get the leaf nodes of the pipeline
    /// Mainly used to get the output nodes when user does not specify them
    pub fn get_leaf_nodes(&self) -> impl Iterator<Item = &Node> {
        // Leaf nodes are those that are not keys in the edges map (i.e., not parents of any node)
        self.graph
            .node_indices()
            .filter(|&node_index| {
                self.graph
                    .neighbors_directed(node_index, Outgoing)
                    .next()
                    .is_none()
            })
            .map(|node_index| &self.graph[node_index])
    }

    /// Function to get the parents of a node
    pub fn get_parents_for_node(&self, node: &Node) -> impl Iterator<Item = &Node> {
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

    /// Function to get the children of a node
    pub fn get_children_for_node(&self, node: &Node) -> impl Iterator<Item = &Node> {
        // Find the NodeIndex for the given node_key
        let node_index = self
            .graph
            .node_indices()
            .find(|&idx| self.graph[idx] == *node);
        node_index.into_iter().flat_map(move |idx| {
            self.graph
                .neighbors_directed(idx, Outgoing)
                .map(move |child_idx| &self.graph[child_idx])
        })
    }
}

#[derive(uniffi::Object, Display, Debug, Clone, Serialize)]
#[display("{self:#?}")]
#[uniffi::export(Display)]
/// `PipelineJob` struct
/// This struct is used to store the pipeline and the input map
pub struct PipelineJob {
    /// Used to unique identify the pipeline job
    pub hash: String,
    /// Pipeline struct (Note: Due to the removal of the hash system to be deferred, this has no guarantee of being unique)
    #[serde(skip)]
    pub pipeline: Pipeline,
    /// Mapping of outside input to keys to be match with the pipeline `input_map`
    pub input_packets: Vec<HashMap<String, PathSet>>,
    /// Directory where to store the outputs of the pipeline
    pub output_dir: URI,
    /// Annotation for the pipeline job
    pub annotation: Option<Annotation>,
}

impl PipelineJob {
    /// New function for pipeline job
    /// # Errors
    /// Error out if there are missing keys or failed to convert to yaml
    pub fn new(
        pipeline: Pipeline,
        input_packets: Vec<HashMap<String, PathSet>>,
        output_dir: URI,
        annotation: Option<Annotation>,
    ) -> Result<Self> {
        // Create the job without_hash
        let no_hash = Self {
            hash: String::new(),
            pipeline,
            input_packets,
            annotation,
            output_dir,
        };

        Ok(Self {
            hash: hash_buffer(to_yaml(&no_hash)?),
            ..no_hash
        })
    }
}

#[derive(uniffi::Object, Debug, Clone, Serialize)]
/// `PipelineResult` struct
/// This struct is used to return the result of a pipeline job
pub struct PipelineResult {
    /// Ref to the pipeline job that was executed
    pub pipeline_job: PipelineJob,
    /// Output packets produced by the pipeline job
    pub output_packets: HashMap<String, Vec<HashMap<String, PathSet>>>,
}
