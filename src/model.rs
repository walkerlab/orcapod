use crate::util::{get_struct_name, hash};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_yaml::{Mapping, Value};
use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    io::{BufRead, BufReader},
    path::PathBuf,
};

pub fn to_yaml<T: Serialize>(instance: &T) -> Result<String, Box<dyn Error>> {
    let mapping: BTreeMap<String, Value> = serde_yaml::from_str(&serde_yaml::to_string(instance)?)?; // sort
    let mut yaml = serde_yaml::to_string(
        &mapping
            .into_iter()
            .filter(|(k, _)| k != "annotation" && k != "hash")
            .collect::<BTreeMap<_, _>>(),
    )?; // skip fields
    yaml.insert_str(0, &format!("class: {}\n", get_struct_name::<T>()?)); // replace class at top

    Ok(yaml)
}

pub fn from_yaml<T: DeserializeOwned>(
    annotation_file: &PathBuf,
    spec_file: &PathBuf,
    hash: &str,
) -> Result<T, Box<dyn Error>> {
    let annotation: Mapping = serde_yaml::from_str(&fs::read_to_string(&annotation_file)?)?;
    let spec_yaml = BufReader::new(fs::File::open(&spec_file)?)
        .lines()
        .skip(1)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .join("\n");

    let mut spec_mapping: BTreeMap<String, Value> = serde_yaml::from_str(&spec_yaml)?;
    spec_mapping.insert("annotation".to_string(), Value::from(annotation));
    spec_mapping.insert("hash".to_string(), Value::from(hash));

    let instance: T = serde_yaml::from_str(&serde_yaml::to_string(&spec_mapping)?)?;
    Ok(instance)
}

// --- core model structs ---

#[derive(Serialize, Deserialize, Debug)]
pub struct Pod {
    pub annotation: Annotation,
    pub hash: String,
    source: String,
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
        name: String,
        version: String,
        description: String,
        source_commit: String,
        image: String,
        command: String,
        input_stream_map: BTreeMap<String, StreamInfo>,
        output_dir: PathBuf,
        output_stream_map: BTreeMap<String, StreamInfo>,
        recommended_cpus: f32,
        recommended_memory: u64,
        required_gpu: Option<GPURequirement>,
    ) -> Result<Self, Box<dyn Error>> {
        let pod_no_hash = Self {
            annotation: Annotation {
                name,
                version,
                description,
            },
            hash: String::new(),
            source: source_commit,
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
            hash: hash(&to_yaml::<Pod>(&pod_no_hash)?),
            ..pod_no_hash
        })
    }
}

// --- util types ---

#[derive(Serialize, Deserialize, Debug)]
pub struct Annotation {
    pub name: String,
    pub version: String,
    pub description: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GPURequirement {
    pub model: GPUModel,
    pub recommended_memory: u64,
    pub count: u16,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GPUModel {
    NVIDIA(String),
    AMD(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StreamInfo {
    pub path: PathBuf,
    pub match_pattern: String,
}
