use crate::{
    crypto::hash_bytes,
    error::Result,
    orchestrator::Status,
    store::DataStore,
    util::{get_type_name, hash},
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_yaml;
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    result,
};
/// Converts a model instance into a consistent yaml.
///
/// # Errors
///
/// Will return `Err` if there is an issue converting an `instance` into YAML (w/o annotation).
pub fn to_yaml<T: Serialize>(instance: &T) -> Result<String> {
    let mut yaml = serde_yaml::to_string(instance)?;
    yaml.insert_str(0, &format!("class: {}\n", get_type_name::<T>())); // replace class at top

    Ok(yaml)
}

// --- core model structs ---

/// A reusable, containerized computational unit.
#[derive(Serialize, Deserialize, Debug, PartialEq, Default, Clone)]
pub struct Pod {
    /// Metadata that doesn't affect reproducibility.
    #[serde(skip)]
    pub annotation: Option<Annotation>,
    /// Unique id based on reproducibility.
    #[serde(skip)]
    pub hash: String,
    /// Reproducible environment for compute.
    pub image: String,
    /// Space-delimited shell command to begin computation.
    pub command: String,
    /// A dictionary of named input streams.
    pub input_stream_map: BTreeMap<String, StreamInfo>,
    /// Absolute output directory within the environment.
    pub output_dir: PathBuf,
    output_stream_map: BTreeMap<String, StreamInfo>,
    source_commit_url: String,
    recommended_cpus: f32,
    recommended_memory: u64,
    required_gpu: Option<GPURequirement>,
}

impl Pod {
    /// Construct a new pod instance.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue initializing a `Pod` instance.
    pub fn new(
        annotation: Option<Annotation>,
        image: String,
        command: String,
        input_stream_map: BTreeMap<String, StreamInfo>,
        output_dir: PathBuf,
        output_stream_map: BTreeMap<String, StreamInfo>,
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
            input_stream_map,
            output_dir,
            output_stream_map,
            source_commit_url,
            recommended_cpus,
            recommended_memory,
            required_gpu,
        };
        Ok(Self {
            hash: hash_bytes(to_yaml(&pod_no_hash)?),
            ..pod_no_hash
        })
    }
}

fn serialize_pod<S>(pod: &Pod, serializer: S) -> result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&pod.hash)
}

fn deserialize_pod<'de, D>(deserializer: D) -> result::Result<Pod, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Pod {
        hash: String::deserialize(deserializer)?,
        ..Pod::default()
    })
}

/// A compute job that specifies resource requests and input/output targets.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default)]
pub struct PodJob {
    /// Metadata that doesn't affect reproducibility.
    #[serde(skip)]
    pub annotation: Option<Annotation>,
    /// Unique id based on reproducibility.
    #[serde(skip)]
    pub hash: String,
    /// A pod to base the pod job on.
    #[serde(serialize_with = "serialize_pod", deserialize_with = "deserialize_pod")]
    pub pod: Pod,
    /// Map stream ids to an input in user data target.
    pub input_stream_mapping: BTreeMap<String, Input>,
    /// Map output directory to a folder in user data target.
    pub output_stream_path: PathBuf,
    /// Maximum allowable cores in fractional cores for the computation.
    pub cpu_limit: f32,
    /// Maximum allowable memory in bytes for the computation.
    pub memory_limit: u64,
    /// Environment variables to be set in environment.
    pub env_vars: Option<HashMap<String, String>>,
    /// Policy on how to handle retry
    pub retry_policy: RetryPolicy,
}

/// An interface to access BLOB functions.
pub trait BlobInterface {
    /// How to evaluate a checksum of a BLOB.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue computing the checksum of a BLOB.
    fn compute_checksum(&self, blob: Blob<FileOrFolder>) -> Result<Blob<FileOrFolder>> {
        Ok(blob)
    }
}

impl PodJob {
    /// Construct a new pod job instance.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue initializing a `PodJob` instance.
    pub fn new(
        annotation: Option<Annotation>,
        pod: Pod,
        input_stream_mapping: BTreeMap<String, Input>,
        output_stream_path: PathBuf,
        cpu_limit: f32,
        memory_limit: u64,
        env_vars: Option<HashMap<String, String>>,
        retry_policy: RetryPolicy,
    ) -> Result<Self> {
        let pod_job_no_hash = Self {
            annotation,
            hash: String::new(),
            pod,
            input_stream_mapping,
            output_stream_path,
            cpu_limit,
            memory_limit,
            env_vars,
            retry_policy,
        };
        Ok(Self {
            hash: hash(to_yaml(&pod_job_no_hash)?),
            ..pod_job_no_hash
        })
    }

    /// Helper function to compute the hash for the `input_stream_path` that is typically used when saving the model
    /// This is using last min save, basically do not compute the checksum, until it is actually written, but can be used
    /// before hand in memory.
    ///
    /// # Errors
    /// Error if fails to compute checksum, mainly due to `FileIO`
    ///
    pub fn compute_checksum_for_input_stream_path<T: DataStore>(
        &mut self,
        model_store: &T,
    ) -> Result<()> {
        for input in self.input_stream_mapping.values_mut() {
            input.compute_checksum(model_store)?;
        }
        Ok(())
    }
}
fn serialize_pod_job<S>(pod_job: &PodJob, serializer: S) -> result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&pod_job.hash)
}

fn deserialize_pod_job<'de, D>(deserializer: D) -> result::Result<PodJob, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(PodJob {
        hash: String::deserialize(deserializer)?,
        ..PodJob::default()
    })
}

/// Result from a compute job run.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PodResult {
    /// Metadata that doesn't affect reproducibility.
    #[serde(skip)]
    pub annotation: Option<Annotation>,
    /// Unique id based on reproducibility.
    #[serde(skip)]
    pub hash: String,
    /// A pod job that originated the pod result.
    #[serde(
        serialize_with = "serialize_pod_job",
        deserialize_with = "deserialize_pod_job"
    )]
    pub pod_job: PodJob,
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
        pod_job: PodJob,
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
            hash: hash(to_yaml(&pod_result_no_hash)?),
            ..pod_result_no_hash
        })
    }
}

// --- util types ---

/// Standard metadata structure for all model instances.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Eq, Clone)]
pub struct Annotation {
    /// A unique name.
    pub name: String,
    /// A unique semantic version.
    pub version: String,
    /// A long form description.
    pub description: String,
}
/// Specification for GPU requirements in computation.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct GPURequirement {
    /// GPU model specification.
    pub model: GPUModel,
    /// Manufacturer recommended memory.
    pub recommended_memory: u64,
    /// Number of GPU cards required.
    pub count: u16,
}
/// GPU model specification.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum GPUModel {
    /// NVIDIA-manufactured card where `String` is the specific model e.g. ???
    NVIDIA(String),
    /// AMD-manufactured card where `String` is the specific model e.g. ???
    AMD(String),
}
/// Streams are named and represent an abstraction for the file(s) that represent some particular
/// data.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct StreamInfo {
    /// Path to stream file.
    pub path: PathBuf,
    /// Naming pattern for the stream.
    pub match_pattern: String,
}
/// Input options sourced from user data target.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum Input {
    /// A single BLOB.
    Unary(Blob<FileOrFolder>),
    /// A series of BLOBs.
    Collection(Vec<Blob<FileOrFolder>>),
}

impl Input {
    fn compute_checksum<T: DataStore>(&mut self, model_store: &T) -> Result<()> {
        match self {
            Self::Unary(blob) => blob.compute_checksum(model_store),
            Self::Collection(blobs) => {
                for blob in blobs {
                    blob.compute_checksum(model_store)?;
                }

                Ok(())
            }
        }
    }
}

/// BLOB in user data target with metadata.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct Blob<T> {
    /// BLOB available options.
    pub kind: T,
    /// BLOB location.
    pub location: PathBuf,
    /// BLOB contents checksum.
    pub checksum: Option<String>,
}

impl Blob<FileOrFolder> {
    // Function to compute the checksum based with handling for default case
    /// We are assuming the datastore to always be the same as model store for now
    /// until store pointer is implmented
    fn compute_checksum<T: DataStore>(&mut self, model_store: &T) -> Result<()> {
        self.checksum = Some(model_store.compute_checksum(&self.location)?);
        Ok(())
    }
}

/// File or folder options for BLOBs.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum FileOrFolder {
    /// A single file specified by its absolute path.
    File,
    /// A single folder specified by its absolute path.
    Folder,
}
/// Folder-only option for BLOBs.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub enum FolderOnly {
    /// A single folder specified by its absolute path.
    #[default]
    Folder,
}

/// Pod job retry policy
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Default)]
pub enum RetryPolicy {
    /// Will stop the job upon first failure
    #[default]
    NoRetry,
    /// Will allow n number of failures within a time window of t seconds
    RetryTimeWindow(u16, u64), // Where u16 is num of retries and u64 is time in seconds
}
