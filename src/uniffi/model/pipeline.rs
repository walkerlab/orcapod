use crate::{
    core::{
        crypto::{hash_buffer, make_random_hash},
        graph::make_graph,
        model::{pipeline::PipelineNode, to_yaml},
        validation::validate_packet,
    },
    uniffi::{
        error::Result,
        model::{
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
use std::{collections::HashMap, path::PathBuf, sync::Arc};
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
    pub input_spec: HashMap<String, Vec<SpecURI>>,
    /// Exposed, internal output specification. Each output is associated with only one node/key.
    pub output_spec: HashMap<String, SpecURI>,
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
        metadata: HashMap<String, Kernel>,
        input_spec: &HashMap<String, Vec<SpecURI>>,
        output_spec: &HashMap<String, SpecURI>,
    ) -> Result<Self> {
        let graph = make_graph(graph_dot, metadata)?;
        Ok(Self {
            graph,
            input_spec: input_spec.clone(),
            output_spec: output_spec.clone(),
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
        output_dir: &URI,
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
            output_dir: output_dir.clone(),
        })
    }
}

impl PipelineJob {
    pub(crate) fn get_input_packets(&self) -> impl Iterator<Item = HashMap<String, PathSet>> {
        let (keys, values) = self
            .input_packet
            .iter()
            .map(|(key, value)| (key.clone(), value))
            .collect::<(Vec<_>, Vec<_>)>();

        values
            .into_iter()
            .multi_cartesian_product()
            .map(move |combo| {
                keys.clone()
                    .into_iter()
                    .zip(combo.into_iter().cloned())
                    .collect::<HashMap<_, _>>()
            })
    }
}

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
pub struct SpecURI {
    /// Node reference name in pipeline.
    pub node_name: String,
    /// Specification key.
    pub key: String,
}
