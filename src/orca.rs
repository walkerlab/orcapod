use std::{collections::BTreeMap, path::PathBuf};

use crate::{
    filestore::FileStore,
    model::{Annotation, GPUSpecRequirement, KeyInfo, Pod},
    store::Store,
};

static DEFAULT_RECOMMNEDED_CPUS: f32 = 2f32;
static DEFAULT_MIN_MEMORY: u64 = 8589934592u64;

pub enum StorageBackend {
    FileStore(String),
}

pub fn create_pod(
    name: String,
    description: String,
    version: String,
    input_stream_map: BTreeMap<String, KeyInfo>,
    output_dir: PathBuf,
    output_stream_map: BTreeMap<String, KeyInfo>, // Will be relative path of output_dir
    docker_file_yaml: String,
    source_commit: String,           // Git Commit only for reference
    recommended_cpus: Option<f32>,   // Default to 2
    min_memory: Option<u64>,         // Defaults to 8GB if not defined
    gpu: Option<GPUSpecRequirement>, // Defaults to None
    storage_backend: StorageBackend,
) -> Result<(), String> {
    // Create Annotation
    let annotation = Annotation {
        name: name,
        description: description,
        version: version,
    };

    // Figure out defaults for cpu and memory if need be
    let rec_cpus = match recommended_cpus {
        Some(value) => value,
        None => DEFAULT_RECOMMNEDED_CPUS,
    };

    let min_mem = match min_memory {
        Some(value) => value,
        None => DEFAULT_MIN_MEMORY,
    };

    // Fake build docker image
    let docker_image_hash = "58286373582200391953cd5caebbe672aa64ca7ffc2dd8e7d43a61ecb144729a";

    // Pod creation
    let pod = Pod {
        input_stream_map,
        output_stream_map,
        output_dir,
        annotation,
        image_sha256_hash: docker_image_hash.to_string(),
        recommended_cpus: rec_cpus,
        min_memory: min_mem,
        gpu,
        source_commit,
    };

    match storage_backend {
        StorageBackend::FileStore(data_storage_path) => {
            let store = FileStore::new(data_storage_path.into());
            pod.store(store)
        }
    }
}
