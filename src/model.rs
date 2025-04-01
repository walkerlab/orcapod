use crate::{
    crypto::{hash_blob, hash_buffer},
    error::Result,
    orchestrator::Status,
    util::get_type_name,
};
use heck::ToSnakeCase as _;
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
    yaml.insert_str(
        0,
        &format!("class: {}\n", get_type_name::<T>().to_snake_case()),
    ); // replace class at top

    Ok(yaml)
}

fn serialize_hashmap<S, K: Ord + Serialize, V: Serialize>(
    map: &HashMap<K, V>,
    serializer: S,
) -> result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let sorted = map.iter().collect::<BTreeMap<_, _>>();
    sorted.serialize(serializer)
}

#[expect(clippy::ref_option, reason = "Serde requires this signature.")]
fn serialize_hashmap_option<S, K: Ord + Serialize, V: Serialize>(
    map_option: &Option<HashMap<K, V>>,
    serializer: S,
) -> result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let sorted = map_option
        .as_ref()
        .map(|map| map.iter().collect::<BTreeMap<_, _>>());
    sorted.serialize(serializer)
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
    /// Exposed, internal input streams.
    #[serde(serialize_with = "serialize_hashmap")]
    pub input_stream: HashMap<String, StreamInfo>,
    /// Exposed, internal output directory.
    pub output_dir: PathBuf,
    #[serde(serialize_with = "serialize_hashmap")]
    output_stream: HashMap<String, StreamInfo>,
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
        input_stream: HashMap<String, StreamInfo>,
        output_dir: PathBuf,
        output_stream: HashMap<String, StreamInfo>,
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
            input_stream,
            output_dir,
            output_stream,
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
    /// Attached, external input streams.
    #[serde(serialize_with = "serialize_hashmap")]
    pub input_stream: HashMap<String, Input>,
    /// Attached, external output directory.
    pub output_dir: OrcaPath,
    /// Maximum allowable cores in fractional cores for the computation.
    pub cpu_limit: f32,
    /// Maximum allowable memory in bytes for the computation.
    pub memory_limit: u64,
    /// Environment variables to be set in environment.
    #[serde(serialize_with = "serialize_hashmap_option")]
    pub env_vars: Option<HashMap<String, String>>,
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
        mut input_stream: HashMap<String, Input>,
        output_dir: OrcaPath,
        cpu_limit: f32,
        memory_limit: u64,
        env_vars: Option<HashMap<String, String>>,
        namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<Self> {
        input_stream = input_stream
            .into_iter()
            .map(|(stream_name, stream_input)| match stream_input {
                Input::Unary(blob) => Ok((
                    stream_name,
                    Input::Unary(hash_blob(namespace_lookup, blob)?),
                )),
                Input::Collection(blobs) => Ok((
                    stream_name,
                    Input::Collection(
                        blobs
                            .into_iter()
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
            input_stream,
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
            hash: hash_buffer(to_yaml(&pod_result_no_hash)?),
            ..pod_result_no_hash
        })
    }
}

// --- util types ---

/// Standard metadata structure for all model instances.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
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
    /// Path to stream file or directory.
    pub path: PathBuf,
    /// Naming pattern for the stream.
    pub match_pattern: String,
}
/// Input options.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum Input {
    /// A single BLOB.
    Unary(Blob),
    /// A series of BLOBs.
    Collection(Vec<Blob>),
}
/// Location of BLOB data.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct OrcaPath {
    /// Namespace alias.
    pub namespace: String,
    /// Path within namespace.
    pub path: PathBuf,
}

/// BLOB with metadata.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct Blob {
    /// BLOB available options.
    pub kind: BlobKind,
    /// BLOB location.
    pub location: OrcaPath,
    /// BLOB contents checksum.
    pub checksum: String,
}
/// File or directory options for BLOBs.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub enum BlobKind {
    /// A single file.
    #[default]
    File,
    /// A single directory.
    Directory,
}
