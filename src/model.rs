use crate::util::{get_type_name, hash};
use anyhow::Result;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_yaml::{Mapping, Value};
use std::{collections::BTreeMap, path::PathBuf};

pub fn to_yaml<T: Serialize>(instance: &T) -> Result<String> {
    let mapping: BTreeMap<String, Value> = serde_yaml::from_str(&serde_yaml::to_string(instance)?)?; // sort
    let mut yaml = serde_yaml::to_string(
        &mapping
            .into_iter()
            .filter(|(k, _)| k != "annotation" && k != "hash")
            .collect::<BTreeMap<_, _>>(),
    )?; // skip fields
    yaml.insert_str(0, &format!("class: {}\n", get_type_name::<T>())); // replace class at top

    Ok(yaml)
}

/// Deserialize struct with optional annotation
pub fn from_yaml<T: DeserializeOwned>(
    spec_yaml: &str,
    hash: &str,
    annotation_yaml: Option<&str>,
) -> Result<T> {
    let mut spec_mapping: BTreeMap<String, Value> = serde_yaml::from_str(spec_yaml)?;

    // Insert annotation if there is something
    if let Some(yaml) = annotation_yaml {
        let annotation_map: Mapping = serde_yaml::from_str(yaml)?;
        spec_mapping.insert("annotation".into(), Value::from(annotation_map));
    }
    spec_mapping.insert("hash".to_owned(), Value::from(hash));

    Ok(serde_yaml::from_str(&serde_yaml::to_string(
        &spec_mapping,
    )?)?)
}

// --- core model structs ---

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct Pod {
    pub annotation: Option<Annotation>,
    pub hash: String,
    source_commit_url: String,
    image: String,
    command: String,
    input_stream_map: BTreeMap<String, StreamInfo>,
    output_dir: PathBuf,
    output_stream_map: BTreeMap<String, StreamInfo>,
    recommended_cpus: f32,
    recommended_memory: u64,
    required_gpu: Option<GPURequirement>,
}

impl Pod {
    pub fn new(
        annotation: Annotation,
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
            annotation: Some(annotation),
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
            hash: hash(&to_yaml(&pod_no_hash)?),
            ..pod_no_hash
        })
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PodJob {
    pub annotation: Annotation,
    pub hash: String,
    pub pod: Pod,
    input_volume_map: BTreeMap<PathBuf, PathBuf>,
    output_volume_map: BTreeMap<PathBuf, PathBuf>,
    cpu_limit: f32, // Num of cpu to limit the pod from
    mem_limit: u64, // Bytes to limit memory
    retry_policy: PodRetryPolicy,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum PodRetryPolicy {
    NoRetry,
    RetryTimeWindow(u16, u64), // Where u16 is num of retries and u64 is time in seconds
}

impl PodJob {
    pub fn new(
        annotation: Annotation,
        pod: Pod,
        input_volume_map: BTreeMap<PathBuf, PathBuf>,
        output_volume_map: BTreeMap<PathBuf, PathBuf>,
        cpu_limit: f32,
        mem_limit: u64,
        retry_policy: PodRetryPolicy,
    ) -> Result<Self, Box<dyn Error>> {
        let pod_job_no_hash = PodJob {
            annotation,
            hash: String::new(),
            pod,
            input_volume_map,
            output_volume_map,
            cpu_limit,
            mem_limit,
            retry_policy,
        };

        Ok(Self {
            hash: hash(&to_yaml(&pod_job_no_hash)?),
            ..pod_job_no_hash
        })
    }
}

// --- util types ---

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct Annotation {
    pub name: String,
    pub version: String,
    pub description: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct GPURequirement {
    pub model: GPUModel,
    pub recommended_memory: u64,
    pub count: u16,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum GPUModel {
    NVIDIA(String), // String will be the specific model of the gpu
    AMD(String),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct StreamInfo {
    pub path: PathBuf,
    pub match_pattern: String,
}
