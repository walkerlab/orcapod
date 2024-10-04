use core::f32;
use std::{collections::BTreeMap, path::PathBuf};

use colored::Colorize;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub trait ToYaml {
    fn to_yaml(&self) -> String;
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Annotation {
    pub name: String,
    pub description: String,
    pub version: String,
}

impl ToYaml for Annotation {
    fn to_yaml(&self) -> String {
        let mut yaml_str = serde_yaml::to_string(&self).expect(&format!(
            "{}{}",
            "Failed to seralize: ".bright_red(),
            format!("{:?}", &self)
        ));

        yaml_str.insert_str(0, "class: Annotation\n");
        yaml_str
    }
}

/// String would be the name of the model
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum GPUVendorInfo {
    NVIDIA(String),
    AMD(String),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct GPUSpecRequirement {
    pub vendor_info: GPUVendorInfo,
    pub min_memory: u64,
    pub count: u16,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct KeyInfo {
    pub path: PathBuf,
    pub matching_pattern: String,
}

#[derive(Serialize, Debug)]
pub struct Pod {
    #[serde(skip_serializing)]
    pub annotation: Annotation,
    pub gpu_spec_requirments: Option<GPUSpecRequirement>,
    pub image_digest: String,
    pub input_stream_map: BTreeMap<String, KeyInfo>, // Num of recommneded cpu cores (can be fractional)
    pub min_memory: u64,
    pub output_dir: PathBuf,
    pub output_stream_map: BTreeMap<String, KeyInfo>,
    #[serde(skip_serializing)]
    pub pod_hash: String, // SHA256 docker image hash
    pub recommended_cpus: f32,
    pub source_commit: String, // Git Commit
}

impl ToYaml for Pod {
    fn to_yaml(&self) -> String {
        let mut yaml_str = serde_yaml::to_string(&self).expect(&format!(
            "{}{}",
            "Failed to seralize: ".bright_red(),
            format!("{:?}", self).bright_cyan()
        ));

        yaml_str.insert_str(0, "class: Pod\n");
        yaml_str
    }
}

#[derive(Clone)]
pub struct PodNewConfig {
    pub name: String,
    pub description: String,
    pub version: String,
    pub input_stream_map: BTreeMap<String, KeyInfo>,
    pub output_dir: PathBuf,
    pub output_stream_map: BTreeMap<String, KeyInfo>, // Will be relative path of output_dir
    pub image_name: String,
    pub source_commit: String,         // Git Commit only for reference
    pub recommended_cpus: Option<f32>, // Num of recommneded cpu cores (can be fractional)
    pub min_memory: Option<u64>,       // Bytes
    pub gpu_spec_requirments: Option<GPUSpecRequirement>,
}

impl Pod {
    pub fn new(config: PodNewConfig) -> Pod {
        let recommended_cpus = match config.recommended_cpus {
            Some(value) => value,
            None => 2f32,
        };

        let min_memory = match config.min_memory {
            Some(value) => value,
            None => 4294967296,
        };

        let mut pod = Pod {
            pod_hash: String::new(),
            input_stream_map: config.input_stream_map,
            output_dir: config.output_dir,
            output_stream_map: config.output_stream_map,
            annotation: Annotation {
                name: config.name,
                description: config.description,
                version: config.version,
            },
            image_digest: config.image_name,
            recommended_cpus,
            min_memory: min_memory,
            gpu_spec_requirments: config.gpu_spec_requirments,
            source_commit: config.source_commit,
        };

        let pod_hash = pod.compute_hash();
        pod.pod_hash = pod_hash;

        pod
    }

    pub fn compute_hash(&self) -> String {
        compute_sha_256_hash(&self.to_yaml())
    }

    pub fn verify(&self) -> Result<(), String> {
        let compute_hash = self.compute_hash();
        match compute_hash != self.pod_hash {
            true => {
                // Hash is different, something went wrong
                Err(format!(
                    "{}{}{}{}{}",
                    "Pod should have hash ".bright_red(),
                    &compute_hash.bright_cyan(),
                    " but ".bright_red(),
                    &self.pod_hash.bright_cyan(),
                    " was found".bright_red()
                ))
            }
            false => Ok(()),
        }
    }
}

fn compute_sha_256_hash(data: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:X}", hasher.finalize())
}
