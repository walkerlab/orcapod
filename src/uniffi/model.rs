use crate::{
    core::{
        crypto::{hash_blob, hash_buffer},
        graph::{make_dot, make_graph, make_svg},
        model::{
            deserialize_pod, deserialize_pod_job, serialize_hashmap, serialize_hashmap_option,
            to_yaml,
        },
        util::get,
    },
    uniffi::{
        error::{OrcaError, Result, selector},
        orchestrator::Status,
    },
};
use derive_more::Display;
use getset::CloneGetters;
use hex;
use petgraph::graph::DiGraph;
use rand::{self, RngCore as _};
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
    pub command: Vec<String>,
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
        command: Vec<String>,
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
            .into_iter()
            .map(|(stream_name, stream_input)| match stream_input {
                PathSet::Unary { blob } => Ok((
                    stream_name,
                    PathSet::Unary {
                        blob: hash_blob(namespace_lookup, blob)?,
                    },
                )),
                PathSet::Collection { blobs } => Ok((
                    stream_name,
                    PathSet::Collection {
                        blobs: blobs
                            .into_iter()
                            .map(|blob| hash_blob(namespace_lookup, blob))
                            .collect::<Result<_>>()?,
                    },
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
    /// Produced, external output packet.
    #[serde(default, serialize_with = "serialize_hashmap")]
    pub output_packet: HashMap<String, PathSet>,
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
        namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<Self> {
        let skip_allowed = match status {
            Status::Completed => false,
            Status::Running | Status::Failed { .. } | Status::Unset => true,
        };
        let output_packet = pod_job
            .pod
            .output_spec
            .iter()
            .filter_map(|(packet_key, path_info)| {
                let location = URI {
                    namespace: pod_job.output_dir.namespace.clone(),
                    path: pod_job.output_dir.path.join(&path_info.path),
                };

                let local_location = match get(namespace_lookup, &location.namespace) {
                    Ok(root_path) => root_path.join(&location.path),
                    Err(error) => return Some(Err(error)),
                };

                match (local_location.try_exists(), skip_allowed) {
                    (Ok(false), true) => None,
                    (Err(error), _) => Some(Err(OrcaError::from(error))),
                    (Ok(false), false) => Some(
                        selector::IncompleteOutputPacket {
                            key: packet_key.clone(),
                            namespace: location.namespace.clone(),
                            path: location.path,
                        }
                        .fail()
                        .map_err(OrcaError::from),
                    ),
                    (Ok(true), _) => Some(Ok((
                        packet_key,
                        Blob {
                            kind: if local_location.is_file() {
                                BlobKind::File
                            } else {
                                BlobKind::Directory
                            },
                            location,
                            checksum: String::new(),
                        },
                    ))),
                }
            })
            .map(|result| {
                let (packet_key, blob) = result?;
                Ok((
                    packet_key.clone(),
                    PathSet::Unary {
                        blob: hash_blob(namespace_lookup, blob)?,
                    },
                ))
            })
            .collect::<Result<_>>()?;
        let pod_result_no_hash = Self {
            annotation,
            hash: String::new(),
            pod_job,
            output_packet,
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
    pub graph: DiGraph<String, String>,
    /// Metadata for each kernel referenced in the DAG.
    pub metadata: HashMap<String, Kernel>,
    /// key -> N number of node name / input key i.e. provides a rename feature + forking
    pub input_spec: HashMap<String, Vec<InputSpecURI>>,
    /// if we omit, then they all have to be exposed using the same it currently has (however, this can create collisions, letting user manually do it ensures no collisions but the downside is they could make it less usable by underexposing...), key -> N number of node name / output key i.e. provides a rename feature
    pub output_spec: HashMap<String, OutputSpecURI>,
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
        metadata: &HashMap<String, Kernel>,
        input_spec: &HashMap<String, Vec<InputSpecURI>>,
        output_spec: &HashMap<String, OutputSpecURI>,
    ) -> Result<Self> {
        // todo: need to somehow create/save the join operator but don't want to expose manually creating it to python
        let graph = make_graph(graph_dot)?;
        Ok(Self {
            graph,
            metadata: metadata.clone(),
            input_spec: input_spec.clone(),
            output_spec: output_spec.clone(),
        })
    }
    /// Cast the graph into [DOT](https://graphviz.org/doc/info/lang.html).
    pub fn make_dot(&self) -> String {
        make_dot(&self.graph, &self.metadata)
    }
    /// Render the graph into SVG.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue parsing the graph.
    pub fn make_svg(&self) -> Result<String> {
        make_svg(&self.graph, &self.metadata)
    }
}

/// A compute pipeline job that supplies input/output targets.
#[expect(
    clippy::field_scoped_visibility_modifiers,
    reason = "Temporary until we add hash to PipelineJob."
)]
#[derive(uniffi::Object, Debug, Display, CloneGetters, Deserialize, Serialize)]
#[getset(get_clone, impl_attrs = "#[uniffi::export]")]
#[display("{self:#?}")]
#[uniffi::export(Display)]
pub struct PipelineJob {
    /// todo: replace this with a consistent hash
    #[getset(skip)]
    pub(crate) hash: String,
    /// A pipeline to base the pipeline job on.
    pub pipeline: Arc<Pipeline>,
    /// Attached, external input streams. Applies cartesian product by default.
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
        let input_packet_with_checksum = input_packet
            .iter()
            .map(|(path_set_key, path_sets)| {
                Ok((
                    path_set_key.clone(),
                    path_sets
                        .iter()
                        .map(|path_set| {
                            Ok(match path_set {
                                PathSet::Unary { blob } => PathSet::Unary {
                                    blob: hash_blob(namespace_lookup, blob.clone())?,
                                },
                                PathSet::Collection { blobs } => PathSet::Collection {
                                    blobs: blobs
                                        .iter()
                                        .map(|blob| hash_blob(namespace_lookup, blob.clone()))
                                        .collect::<Result<_>>()?,
                                },
                            })
                        })
                        .collect::<Result<_>>()?,
                ))
            })
            .collect::<Result<_>>()?;

        let mut bytes = [0; 32];
        rand::rng().fill_bytes(&mut bytes);

        Ok(Self {
            hash: hex::encode(bytes),
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
    /// NVIDIA-manufactured card
    NVIDIA {
        /// Model ID
        id: String,
    },
    /// AMD-manufactured card
    AMD {
        /// Model ID
        id: String,
    },
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
    /// A singleton.
    Unary {
        /// Blob info.
        blob: Blob,
    },
    /// A collection.
    Collection {
        /// Blob info for a series of blobs.
        blobs: Vec<Blob>,
    },
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

/// Index from pipeline node into input specification.
#[derive(uniffi::Record, Debug, Clone, Deserialize, Serialize)]
pub struct InputSpecURI {
    /// Node reference name in pipeline.
    pub node_id: String,
    /// Specification key.
    pub key: String,
}

/// Index from pipeline node into output specification.
#[derive(uniffi::Record, Debug, Clone, Deserialize, Serialize)]
pub struct OutputSpecURI {
    /// Node reference name in pipeline.
    pub node_id: String,
    /// Specification key.
    pub key: String,
}

// --- utils ----

uniffi::custom_type!(PathBuf, String, {
    remote,
    try_lift: |val| Ok(PathBuf::from(&val)),
    lower: |obj| obj.display().to_string(),
});
