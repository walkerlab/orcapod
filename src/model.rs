use core::f32;
use std::{collections::BTreeMap, error::Error, path::PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{error::SerializeError, util::get_struct_name};

pub fn to_yaml<T: Serialize + std::fmt::Debug>(
    item: T,
) -> Result<String, Box<dyn Error>> {
    let mut yaml_str = match serde_yaml::to_string(&item) {
        Ok(value) => value,
        Err(error) => {
            return Err(Box::new(SerializeError {
                item_debug_string: format!("{:?}", item),
                error,
            }))
        }
    };

    yaml_str.insert_str(0, &format!("class: {}\n", get_struct_name::<T>()?));

    Ok(yaml_str)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Annotation {
    pub name: String,
    pub description: String,
    pub version: String,
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

#[derive(Clone)]
pub struct PodNewConfig {
    pub name: String,
    pub description: String,
    pub version: String,
    pub input_stream_map: BTreeMap<String, KeyInfo>,
    pub output_dir: PathBuf,
    pub output_stream_map: BTreeMap<String, KeyInfo>, // Will be relative path of output_dir
    pub image_name: String,
    pub source_commit: String, // Git Commit only for reference
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

        // Covert to yaml, if fails, just panic since it should always be valid
        let pod_hash = format!("{:X}", Sha256::digest(to_yaml(&pod).unwrap()));
        pod.pod_hash = pod_hash;

        pod
    }
}
