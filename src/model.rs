use crate::util::{get_struct_name, hash_buffer};
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
    let mapping: BTreeMap<String, Value> =
        serde_yaml::from_str(&serde_yaml::to_string(instance)?)?; // sort
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
    annotation_file: &str,
    spec_file: &str,
    hash: &str,
) -> Result<T, Box<dyn Error>> {
    let annotation: Mapping =
        serde_yaml::from_str(&fs::read_to_string(&annotation_file)?)?;
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
    required_gpu: Option<RequiredGPU>,
    file_content_checksums: BTreeMap<PathBuf, String>,
}

impl Pod {
    pub fn new(
        annotation: Annotation,
        source: String,
        image: String,
        command: String,
        input_stream_map: BTreeMap<String, StreamInfo>,
        output_dir: PathBuf,
        output_stream_map: BTreeMap<String, StreamInfo>,
        recommended_cpus: f32,
        recommended_memory: u64,
        required_gpu: Option<RequiredGPU>,
    ) -> Result<Self, Box<dyn Error>> {
        // todo: resolved_image+checksums -> docker image:tag -> image@digest
        let resolved_image = String::from(
            "zenmldocker/zenml-server@sha256:78efb7728aac9e4e79966bc13703e7cb239ba9c0eb6322c252bea0399ff2421f"
        );
        let checksums = BTreeMap::from([(
            PathBuf::from("image.tar.gz"),
            String::from(
                "78efb7728aac9e4e79966bc13703e7cb239ba9c0eb6322c252bea0399ff2421f",
            ),
        )]);
        let pod_no_hash = Self {
            annotation,
            hash: String::new(),
            source,
            image: resolved_image,
            command,
            input_stream_map,
            output_dir,
            output_stream_map,
            recommended_cpus,
            recommended_memory,
            required_gpu,
            file_content_checksums: checksums,
        };
        Ok(Self {
            hash: hash_buffer(&to_yaml::<Pod>(&pod_no_hash)?),
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
pub struct RequiredGPU {
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
