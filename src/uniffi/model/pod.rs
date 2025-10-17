use crate::{
    core::{
        crypto::{hash_blob, hash_buffer},
        model::{
            pod::{deserialize_pod, deserialize_pod_job},
            serialize_hashmap, serialize_hashmap_option, to_yaml,
        },
        util::get,
        validation::validate_packet,
    },
    uniffi::{
        error::{OrcaError, Result},
        model::{
            Annotation,
            packet::{Blob, BlobKind, Packet, PathInfo, PathSet, URI},
        },
        orchestrator::PodStatus,
    },
};
use derive_more::Display;
use getset::CloneGetters;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use uniffi;

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
    /// Shell command to begin computation. First element is the executable and remaining elements
    /// are the arguments.
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
    pub input_packet: Packet,
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
        mut input_packet: Packet,
        output_dir: URI,
        cpu_limit: f32,
        memory_limit: u64,
        env_vars: Option<HashMap<String, String>>,
        namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<Self> {
        validate_packet("input".into(), &pod.input_spec, &input_packet)?;
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
    /// Produced, external output packet.
    #[serde(serialize_with = "serialize_hashmap")]
    pub output_packet: Packet,
    /// Name given by orchestrator.
    pub assigned_name: String,
    /// Status of compute run when terminated.
    pub status: PodStatus,
    /// Time in epoch when created in seconds.
    pub created: u64,
    /// Time in epoch when terminated in seconds.
    pub terminated: u64,
    /// Logs about stdout and stderr, where stderr is append at the end
    pub logs: String,
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
        status: PodStatus,
        created: u64,
        terminated: u64,
        namespace_lookup: &HashMap<String, PathBuf>,
        logs: String,
    ) -> Result<Self> {
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

                match local_location.try_exists() {
                    Ok(false) => None,
                    Err(error) => Some(Err(OrcaError::from(error))),
                    Ok(true) => Some(Ok((
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
                    PathSet::Unary(hash_blob(namespace_lookup, &blob)?),
                ))
            })
            .collect::<Result<_>>()?;

        // If packet is completed, the output packet must meet the output spec
        if matches!(status, PodStatus::Completed) {
            validate_packet("output".into(), &pod_job.pod.output_spec, &output_packet)?;
        }

        let pod_result_no_hash = Self {
            annotation,
            hash: String::new(),
            pod_job,
            output_packet,
            assigned_name,
            status,
            created,
            terminated,
            logs,
        };
        Ok(Self {
            hash: hash_buffer(to_yaml(&pod_result_no_hash)?),
            ..pod_result_no_hash
        })
    }
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
