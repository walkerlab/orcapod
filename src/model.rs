use std::{collections::BTreeMap, path::PathBuf};

use colored::Colorize;

use crate::store::Store;

pub struct Annotation {
    pub name: String,
    pub description: String,
    pub version: String, // Look into version lib
}

enum GPUVendorInfo {
    NVIDIA(String),
    AMD(String),
}

pub struct GPUSpecRequirement {
    vendor_info: GPUVendorInfo,
    min_memory: u64,
    count: u16,
}

pub struct KeyInfo {
    path: PathBuf,
    matching_pattern: String,
}

pub struct Pod {
    input_stream_map: BTreeMap<String, KeyInfo>,
    output_stream_map: BTreeMap<String, KeyInfo>, // Will be relative path of output_dir
    output_dir: PathBuf,
    annotation: Annotation,
    source: String,       // Git Commit
    image: String,        // SHA256 docker image hash
    recommended_gpu: f32, // Num of recommneded cpu cores (can be fractional)
    min_memory: u64,      // Bytes
    gpu: Option<GPUSpecRequirement>,
}

impl Pod {
    fn store<T: Store>(&self, store: T) {
        match store.store_pod(self) {
            Ok(_) => (),
            Err(e) => panic!("{}{}", "Failed to store pod with error: ".bright_red(), e.to_string().bright_cyan()),
        }
    }
}
