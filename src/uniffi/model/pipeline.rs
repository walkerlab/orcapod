use crate::{
    core::{
        crypto::{hash_blob, make_random_hash},
        graph::make_graph,
        pipeline::PipelineNode,
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

#[expect(clippy::excessive_nesting, reason = "Nesting manageable.")]
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
                        .map(|path_set| {
                            Ok(match path_set {
                                PathSet::Unary(blob) => {
                                    PathSet::Unary(hash_blob(namespace_lookup, blob)?)
                                }
                                PathSet::Collection(blobs) => PathSet::Collection(
                                    blobs
                                        .iter()
                                        .map(|blob| hash_blob(namespace_lookup, blob))
                                        .collect::<Result<_>>()?,
                                ),
                            })
                        })
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

/// A node in a computational pipeline.
#[derive(uniffi::Enum, Debug, Clone, Deserialize, Serialize)]
pub enum Kernel {
    /// Pod reference.
    Pod {
        /// See [`Pod`].
        r#ref: Arc<Pod>,
    },
    /// Cartesian product operation. See [`crate::core::operator::JoinOperator`].
    JoinOperator,
    /// Rename a path set key operation.
    MapOperator {
        /// See [`crate::core::operator::MapOperator`].
        map: HashMap<String, String>,
    },
}

/// Index from pipeline node into pod specification.
#[derive(uniffi::Record, Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SpecURI {
    /// Node reference name in pipeline.
    pub node: String,
    /// Specification key.
    pub key: String,
}
