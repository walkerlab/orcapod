use crate::{
    core::{
        crypto::{hash_buffer, make_random_hash},
        graph::make_graph,
        model::{pipeline::PipelineNode, to_yaml},
        validation::validate_packet,
    },
    uniffi::{
        error::{Kind, OrcaError, Result},
        model::{
            Annotation,
            packet::{PathSet, URI},
            pod::Pod,
        },
    },
};
use derive_more::Display;
use getset::CloneGetters;
use itertools::Itertools as _;
use petgraph::graph::DiGraph;
use serde::{Deserialize, Serialize};
use std::{backtrace::Backtrace, collections::HashMap, path::PathBuf, sync::Arc};
use uniffi;

/// Computational dependencies as a [DAG](https://en.wikipedia.org/wiki/Directed_acyclic_graph).
#[derive(uniffi::Object, Debug, Display, CloneGetters, Clone, Deserialize, Serialize)]
#[getset(get_clone, impl_attrs = "#[uniffi::export]")]
#[display("{self:#?}")]
#[uniffi::export(Display)]
pub struct Pipeline {
    /// Computational DAG in-memory.
    #[getset(skip)]
    pub graph: DiGraph<PipelineNode, ()>,
    /// Exposed, internal input specification. Each input may be fed into more than one node/key if desired.
    pub input_spec: HashMap<String, Vec<NodeURI>>,
    /// Exposed, internal output specification. Each output is associated with only one node/key.
    pub output_spec: HashMap<String, NodeURI>,
    /// Optional annotation for the pipeline.
    #[getset(skip)]
    pub annotation: Option<Annotation>,
}

#[uniffi::export]
impl Pipeline {
    /// Construct a new pipeline instance.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue initializing a `Pipeline` instance.
    #[uniffi::constructor]
    pub fn new(
        graph_dot: &str,
        kernel_map: HashMap<String, Kernel>,
        input_spec: HashMap<String, Vec<NodeURI>>,
        output_spec: HashMap<String, NodeURI>,
        annotation: Option<Annotation>,
    ) -> Result<Self> {
        let graph = make_graph(graph_dot, kernel_map)?;
        Ok(Self {
            graph,
            input_spec,
            output_spec,
            annotation,
        })
    }
}

/// A compute pipeline job that supplies input/output targets.
#[expect(
    clippy::field_scoped_visibility_modifiers,
    reason = "Temporary until a proper hash is implemented."
)]
#[derive(uniffi::Object, Debug, Display, CloneGetters, Deserialize, Serialize, Clone)]
#[getset(get_clone, impl_attrs = "#[uniffi::export]")]
#[display("{self:#?}")]
#[uniffi::export(Display)]
pub struct PipelineJob {
    /// todo: replace with a consistent hash
    #[getset(skip)]
    pub(crate) hash: String,
    /// A pipeline to base the pipeline job on.
    pub pipeline: Arc<Pipeline>,
    /// Attached, external input packet. Applies cartesian product by default on keys pointing to the same node.
    pub input_packet: HashMap<String, Vec<PathSet>>,
    /// Attached, external output directory.
    pub output_dir: URI,
    /// Optional annotation for the pipeline job.
    #[getset(skip)]
    pub annotation: Option<Annotation>,
}

#[uniffi::export]
impl PipelineJob {
    /// Construct a new pipeline job instance.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue initializing a `PipelineJob` instance.
    #[uniffi::constructor]
    pub fn new(
        pipeline: Arc<Pipeline>,
        input_packet: &HashMap<String, Vec<PathSet>>,
        output_dir: URI,
        annotation: Option<Annotation>,
        namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<Self> {
        validate_packet("input".into(), &pipeline.input_spec, input_packet)?;
        let input_packet_with_checksum = input_packet
            .iter()
            .map(|(path_set_key, path_sets)| {
                Ok((
                    path_set_key.clone(),
                    path_sets
                        .iter()
                        .map(|path_set| path_set.hash_content(namespace_lookup))
                        .collect::<Result<_>>()?,
                ))
            })
            .collect::<Result<_>>()?;

        Ok(Self {
            hash: make_random_hash(),
            pipeline,
            input_packet: input_packet_with_checksum,
            output_dir,
            annotation,
        })
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
                    .entry(&node_uri.node_name)
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

/// Struct to hold the result of a pipeline execution.
pub struct PipelineResult {
    /// The pipeline job that was executed.
    pub pipeline_job: Arc<PipelineJob>,
    /// The result of the pipeline execution.
    pub output_packets: HashMap<String, Vec<PathSet>>,
}

/// A node in a computational pipeline.
#[derive(uniffi::Enum, Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum Kernel {
    /// Pod reference.
    Pod {
        /// See [`Pod`].
        pod: Arc<Pod>,
    },
    /// Cartesian product operation. See [`crate::core::operator::JoinOperator`].
    Joiner,
    /// Rename a path set key operation.
    Mapper {
        /// See [`crate::core::operator::MapOperator`].
        mapper: Arc<Mapper>,
    },
}

impl From<Pod> for Kernel {
    fn from(pod: Pod) -> Self {
        Self::Pod { pod: Arc::new(pod) }
    }
}

impl From<Mapper> for Kernel {
    fn from(mapper: Mapper) -> Self {
        Self::Mapper {
            mapper: Arc::new(mapper),
        }
    }
}

/// Mapper struct to store mapping information between input and output stream keys.
#[derive(uniffi::Object, Display, Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
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

/// Index from pipeline node into pod specification.
#[derive(uniffi::Record, Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct NodeURI {
    /// Node reference name in pipeline.
    pub node_name: String,
    /// Specification key.
    pub key: String,
}

#[uniffi::export]
impl NodeURI {
    /// Create a new `NodeURI` instance.
    #[uniffi::constructor]
    pub const fn new(node_name: String, key: String) -> Self {
        Self { node_name, key }
    }
}
