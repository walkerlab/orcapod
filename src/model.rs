use core::f32;
use std::{collections::BTreeMap, path::PathBuf, process::Output};

use colored::Colorize;
use serde::{Deserialize, Serialize};

use crate::store::Store;

pub struct Annotation {
    pub name: String,
    pub description: String,
    pub version: String, // Look into version lib
}

#[derive(Serialize, Deserialize, Debug, Clone)]
enum GPUVendorInfo {
    NVIDIA(String),
    AMD(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GPUSpecRequirement {
    pub vendor_info: GPUVendorInfo,
    pub min_memory: u64,
    pub count: u16,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct KeyInfo {
    pub path: PathBuf,
    pub matching_pattern: String,
}

pub struct Pod {
    pub input_stream_map: BTreeMap<String, KeyInfo>,
    pub output_dir: PathBuf,
    pub output_stream_map: BTreeMap<String, KeyInfo>, // Will be relative path of output_dir
    pub annotation: Annotation,
    pub image_sha256_hash: String, // SHA256 docker image hash
    pub recommended_cpus: f32,     // Num of recommneded cpu cores (can be fractional)
    pub min_memory: u64,           // Bytes
    pub gpu: Option<GPUSpecRequirement>,
    pub source_commit: String, // Git Commit
}

impl Pod {
    pub fn store<T: Store>(&self, store: T) -> Result<(), String> {
        store.store_pod(self)
    }
}
