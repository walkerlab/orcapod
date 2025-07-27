use crate::{
    core::{
        crypto::{hash_blob, hash_buffer, make_random_hash},
        graph::make_graph,
        model::{
            deserialize_pod, deserialize_pod_job, serialize_hashmap, serialize_hashmap_option,
            to_yaml,
        },
        pipeline::PipelineNode,
    },
    uniffi::{error::Result, orchestrator::Status},
};
use derive_more::Display;
use getset::CloneGetters;
use petgraph::graph::DiGraph;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use uniffi;

/// Available models.
#[derive(uniffi::Enum, Debug)]
pub enum ModelType {
    /// See [`Pod`].
    Pod,
    /// See [`PodJob`].
    PodJob,
    /// See [`PodResult`].
    PodResult,
}

// --- core model structs ---

/// A reusable, containerized computational unit.
#[derive(
    uniffi::Object, Serialize, Deserialize, Debug, PartialEq, Default, Clone, Display, CloneGetters,
)]
#[getset(get_clone, impl_attrs = "#[uniffi::export]")]
#[display("{self:#?}")]
#[uniffi::export(Display)]
pub struct Pod {
    /// Metadata that doesn't affect reproducibility.
    pub annotation: Option<Annotation>,
    /// Unique id based on reproducibility.
    #[serde(default)]
    pub hash: String,
    /// Reproducible environment for compute.
    pub image: String,
    /// Space-delimited shell command to begin computation.
    pub command: String,
    /// Exposed, internal input specification.
    #[serde(serialize_with = "serialize_hashmap")]
    pub input_spec: HashMap<String, PathInfo>,
    /// Exposed, internal output directory.
    pub output_dir: PathBuf,
    /// Exposed, internal output specification.
    #[serde(serialize_with = "serialize_hashmap")]
    pub output_spec: HashMap<String, PathInfo>,
    /// Link to source associated with image binary.
    pub source_commit_url: String,
    /// Recommendation for CPU in fractional cores.
    pub recommended_cpus: f32,
    /// Recommendation for memory in bytes.
    pub recommended_memory: u64,
    /// If applicable, recommendation for GPU configuration.
    pub required_gpu: Option<GPURequirement>,
}

#[uniffi::export]
impl Pod {
    /// Construct a new pod instance.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue initializing a `Pod` instance.
    #[uniffi::constructor]
    pub fn new(
        annotation: Option<Annotation>,
        image: String,
        command: String,
        input_spec: HashMap<String, PathInfo>,
        output_dir: PathBuf,
        output_spec: HashMap<String, PathInfo>,
        source_commit_url: String,
        recommended_cpus: f32,
        recommended_memory: u64,
        required_gpu: Option<GPURequirement>,
    ) -> Result<Self> {
        let pod_no_hash = Self {
            annotation,
            hash: String::new(),
            image,
            command,
            input_spec,
            output_dir,
            output_spec,
            source_commit_url,
            recommended_cpus,
            recommended_memory,
            required_gpu,
        };
        Ok(Self {
            hash: hash_buffer(to_yaml(&pod_no_hash)?),
            ..pod_no_hash
        })
    }
}

/// A compute job that specifies resource requests and input/output targets.
#[derive(
    uniffi::Object, Serialize, Deserialize, Debug, PartialEq, Clone, Default, Display, CloneGetters,
)]
#[getset(get_clone, impl_attrs = "#[uniffi::export]")]
#[display("{self:#?}")]
#[uniffi::export(Display)]
pub struct PodJob {
    /// Metadata that doesn't affect reproducibility.
    pub annotation: Option<Annotation>,
    /// Unique id based on reproducibility.
    #[serde(default)]
    pub hash: String,
    /// A pod to base the pod job on.
    #[serde(deserialize_with = "deserialize_pod")]
    pub pod: Arc<Pod>,
    /// Attached, external input packet.
    #[serde(serialize_with = "serialize_hashmap")]
    pub input_packet: HashMap<String, PathSet>,
    /// Attached, external output directory.
    pub output_dir: URI,
    /// Maximum allowable cores in fractional cores for the computation.
    pub cpu_limit: f32,
    /// Maximum allowable memory in bytes for the computation.
    pub memory_limit: u64,
    /// Environment variables to be set in environment.
    #[serde(serialize_with = "serialize_hashmap_option")]
    pub env_vars: Option<HashMap<String, String>>,
}

#[uniffi::export]
impl PodJob {
    /// Construct a new pod job instance.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue initializing a `PodJob` instance.
    #[uniffi::constructor]
    pub fn new(
        annotation: Option<Annotation>,
        pod: Arc<Pod>,
        mut input_packet: HashMap<String, PathSet>,
        output_dir: URI,
        cpu_limit: f32,
        memory_limit: u64,
        env_vars: Option<HashMap<String, String>>,
        namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<Self> {
        input_packet = input_packet
            .iter()
            .map(|(stream_name, stream_input)| match stream_input {
                PathSet::Unary(blob) => Ok((
                    stream_name.clone(),
                    PathSet::Unary(hash_blob(namespace_lookup, blob)?),
                )),
                PathSet::Collection(blobs) => Ok((
                    stream_name.clone(),
                    PathSet::Collection(
                        blobs
                            .iter()
                            .map(|blob| hash_blob(namespace_lookup, blob))
                            .collect::<Result<Vec<_>>>()?,
                    ),
                )),
            })
            .collect::<Result<_>>()?;
        let pod_job_no_hash = Self {
            annotation,
            hash: String::new(),
            pod,
            input_packet,
            output_dir,
            cpu_limit,
            memory_limit,
            env_vars,
        };
        Ok(Self {
            hash: hash_buffer(to_yaml(&pod_job_no_hash)?),
            ..pod_job_no_hash
        })
    }
}

/// Result from a compute job run.
#[derive(uniffi::Record, Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct PodResult {
    /// Metadata that doesn't affect reproducibility.
    pub annotation: Option<Annotation>,
    /// Unique id based on reproducibility.
    #[serde(default)]
    pub hash: String,
    /// A pod job that originated the pod result.
    #[serde(deserialize_with = "deserialize_pod_job")]
    pub pod_job: Arc<PodJob>,
    /// Name given by orchestrator.
    pub assigned_name: String,
    /// Status of compute run when terminated.
    pub status: Status,
    /// Time in epoch when created in seconds.
    pub created: u64,
    /// Time in epoch when terminated in seconds.
    pub terminated: u64,
}

impl PodResult {
    /// Construct a new pod result instance.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue initializing a `PodResult` instance.
    pub fn new(
        annotation: Option<Annotation>,
        pod_job: Arc<PodJob>,
        assigned_name: String,
        status: Status,
        created: u64,
        terminated: u64,
    ) -> Result<Self> {
        let pod_result_no_hash = Self {
            annotation,
            hash: String::new(),
            pod_job,
            assigned_name,
            status,
            created,
            terminated,
        };
        Ok(Self {
            hash: hash_buffer(to_yaml(&pod_result_no_hash)?),
            ..pod_result_no_hash
        })
    }
}

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

// --- util types ---

/// Standard metadata structure for all model instances.
#[derive(uniffi::Record, Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct Annotation {
    /// A unique name.
    pub name: String,
    /// A unique semantic version.
    pub version: String,
    /// A long form description.
    pub description: String,
}
/// Specification for GPU requirements in computation.
#[derive(uniffi::Record, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct GPURequirement {
    /// GPU model specification.
    pub model: GPUModel,
    /// Manufacturer recommended memory.
    pub recommended_memory: u64,
    /// Number of GPU cards required.
    pub count: u16,
}
/// GPU model specification.
#[derive(uniffi::Enum, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum GPUModel {
    /// NVIDIA-manufactured card where `String` is the specific model e.g. ???
    NVIDIA(String),
    /// AMD-manufactured card where `String` is the specific model e.g. ???
    AMD(String),
}
/// Streams are named and represent an abstraction for the file(s) that represent some particular
/// data.
#[derive(uniffi::Record, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PathInfo {
    /// Path to stream file or directory.
    pub path: PathBuf,
    /// Naming pattern for the stream.
    pub match_pattern: String,
}
/// A single BLOB or a collection of BLOBs.
#[derive(uniffi::Enum, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum PathSet {
    /// A single BLOB.
    Unary(Blob),
    /// A series of BLOBs.
    Collection(Vec<Blob>),
}
/// Location of BLOB data.
#[derive(uniffi::Record, Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct URI {
    /// Namespace alias.
    pub namespace: String,
    /// Path within namespace.
    pub path: PathBuf,
}

/// BLOB with metadata.
#[derive(uniffi::Record, Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct Blob {
    /// BLOB available options.
    pub kind: BlobKind,
    /// BLOB location.
    pub location: URI,
    /// BLOB contents checksum.
    pub checksum: String,
}
/// File or directory options for BLOBs.
#[derive(uniffi::Enum, Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub enum BlobKind {
    /// A single file.
    #[default]
    File,
    /// A single directory.
    Directory,
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

// --- utils ----

uniffi::custom_type!(PathBuf, String, {
    remote,
    try_lift: |val| Ok(PathBuf::from(&val)),
    lower: |obj| obj.display().to_string(),
});
