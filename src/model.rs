use crate::{
    error::Result,
    util::{get_type_name, hash},
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{collections::BTreeMap, path::PathBuf, result};
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
    image: String,
    command: String,
    input_stream_map: BTreeMap<String, StreamInfo>,
    output_dir: PathBuf,
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
        source_commit_url: String,
        image: String,
        command: String,
        input_stream_map: BTreeMap<String, StreamInfo>,
        output_dir: PathBuf,
        output_stream_map: BTreeMap<String, StreamInfo>,
        recommended_cpus: f32,
        recommended_memory: u64,
        required_gpu: Option<GPURequirement>,
    ) -> Result<Self> {
        let pod_no_hash = Self {
            annotation,
            hash: String::new(),
            source_commit_url,
            image,
            command,
            input_stream_map,
            output_dir,
            output_stream_map,
            recommended_cpus,
            recommended_memory,
            required_gpu,
        };
        Ok(Self {
            hash: hash(to_yaml(&pod_no_hash)?),
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
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
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
    cpu_limit: f32,
    memory_limit: u64,
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
        cpu_limit: f32,
        memory_limit: u64,
    ) -> Result<Self> {
        let pod_job_no_hash = Self {
            annotation,
            hash: String::new(),
            pod,
            cpu_limit,
            memory_limit,
        };
        Ok(Self {
            hash: hash(to_yaml(&pod_job_no_hash)?),
            ..pod_job_no_hash
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
    /// Path to stream file.
    pub path: PathBuf,
    /// Naming pattern for the stream.
    pub match_pattern: String,
}
