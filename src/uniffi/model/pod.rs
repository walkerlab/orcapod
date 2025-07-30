use crate::{
    core::{
        crypto::{hash_buffer, hash_dir, hash_file},
        model::{
            pod::{deserialize_pod, deserialize_pod_job},
            serialize_hashmap, serialize_hashmap_option, to_yaml,
        },
        util::get,
        validation::validate_packet,
    },
    uniffi::{
        error::{Kind, OrcaError, Result},
        model::{
            Annotation,
            packet::{Blob, BlobKind, PathInfo, PathSet, URI},
        },
        orchestrator::Status,
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
        validate_packet("input".into(), &pod.input_spec, &input_packet)?;
        // Hash all the pathset in the input_packet
        input_packet = input_packet
            .iter()
            .map(|(path_set_key, path_set)| {
                Ok((
                    path_set_key.clone(),
                    path_set.hash_content(namespace_lookup)?,
                ))
            })
            .collect::<Result<HashMap<_, _>>>()?;
        // Build the pod job
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

    /// Util function to get the `output_packet` from a given `pod_job`, assuming it results already computed
    /// # Errors
    /// Will return `Err` if the output packet cannot be constructed, e.g. if the pod job has not been run yet or the output directory is not set.
    pub fn get_output_packet(
        &self,
        namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<HashMap<String, PathSet>> {
        self.pod
            .output_spec
            .iter()
            .map(|(key, value)| {
                // Construct the full path and figure out if it is a file or directory
                let namespace_path = get(namespace_lookup, &self.output_dir.namespace)?;
                let rel_path = self.output_dir.path.join(&value.path);
                let abs_path = namespace_path.join(&rel_path);

                // Check if if it is a file or directory
                let path_set = if abs_path.is_file() {
                    PathSet::Unary(Blob {
                        kind: BlobKind::File,
                        location: URI {
                            namespace: self.output_dir.namespace.clone(),
                            path: rel_path,
                        },
                        checksum: hash_file(&abs_path)?,
                    })
                } else if abs_path.is_dir() {
                    PathSet::Unary(Blob {
                        kind: BlobKind::Directory,
                        location: URI {
                            namespace: self.output_dir.namespace.clone(),
                            path: rel_path,
                        },
                        checksum: hash_dir(&abs_path)?,
                    })
                } else {
                    return Err(OrcaError {
                        kind: Kind::UnsupportedPathType {
                            path: abs_path,
                            backtrace: Some(snafu::Backtrace::capture()),
                        },
                    });
                };
                Ok((key.clone(), path_set))
            })
            .collect::<Result<_>>()
    }
}

#[derive(uniffi::Enum, Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
/// Status of a pod result.
pub enum PodResultStatus {
    /// Pod Job completed successfully.
    Completed,
    /// Pod Job failed with an exit code.
    Failed(i16),
    /// Mainly used for default values, not a valid status.
    #[default]
    Unset,
}

impl TryFrom<Status> for PodResultStatus {
    type Error = OrcaError;

    fn try_from(status: Status) -> Result<Self, Self::Error> {
        match status {
            Status::Completed => Ok(Self::Completed),
            Status::Failed(code) => Ok(Self::Failed(code)),
            Status::Running | Status::Unset => Err(OrcaError {
                kind: Kind::StatusConversionFailure {
                    status,
                    reason: "Cannot convert Running or Unset status to PodResultStatus".to_owned(),
                    backtrace: Some(snafu::Backtrace::capture()),
                },
            }),
        }
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
    pub status: PodResultStatus,
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
        status: PodResultStatus,
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
