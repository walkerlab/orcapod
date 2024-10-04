use std::{collections::BTreeMap, default, fs, path::PathBuf};

use orcapod::{
    model::{KeyInfo, Pod, PodNewConfig},
    orca::{create_pod, load_pod, StorageBackend},
};

static TEST_DATA_STORAGE_FOLDER_NAME: &str = "orca-data-test";

/// Base on the style transfer use case with 2 inputs and 1 output
/// In reality doesn't really do much but add the two image together image + style
#[test]
fn test_pod() {
    let storage_backend = StorageBackend::FileStore(TEST_DATA_STORAGE_FOLDER_NAME.to_string());

    let mut input_stream_map = BTreeMap::<String, KeyInfo>::new();

    input_stream_map.insert(
        "input_1".into(),
        KeyInfo {
            path: "/image.png".into(),
            matching_pattern: "*.png".into(),
        },
    );

    input_stream_map.insert(
        "input_2".into(),
        KeyInfo {
            path: "/style.png".into(),
            matching_pattern: "*.png".into(),
        },
    );

    let mut output_stream_map = BTreeMap::<String, KeyInfo>::new();

    output_stream_map.insert(
        "final_img".into(),
        KeyInfo {
            path: "image.png".into(),
            matching_pattern: "*.png".into(),
        },
    );

    let output_dir = PathBuf::from("/output");

    // Create the pod and store it
    let config = PodNewConfig {
        name: "Style Transfer Test Pod".into(),
        description: "Part of orca test suite".into(),
        version: "0.0.0".into(),
        input_stream_map,
        output_dir,
        output_stream_map,
        image_name: "hello-world".to_string(),
        source_commit: "asjdklf8921jfoija0s9f1".into(),
        recommended_cpus: None,
        min_memory: None,
        gpu_spec_requirments: None,
    };

    let pod = match create_pod(config.clone(), &storage_backend) {
        Ok(value) => value,
        Err(e) => panic!("{}", e),
    };

    check_pod_against_config(&pod, &config);

    // Try loading it now from storage noww
    let pod = match load_pod(&pod.pod_hash, &storage_backend) {
        Ok(value) => value,
        Err(e) => panic!("{}", e),
    };

    check_pod_against_config(&pod, &config);

    // Clean up
    fs::remove_dir_all(TEST_DATA_STORAGE_FOLDER_NAME).unwrap();
}

fn check_pod_against_config(pod: &Pod, config: &PodNewConfig) {
    // need to better design this, kinda of lazy at the moment, come back to it later
    assert!(pod.annotation.name == config.name);
    assert!(pod.annotation.description == config.description);
    assert!(pod.annotation.version == config.version);
    assert!(pod.input_stream_map == config.input_stream_map);
    assert!(pod.output_dir == config.output_dir);
    assert!(pod.output_stream_map == config.output_stream_map);
    assert!(pod.source_commit == config.source_commit);
    assert!(pod.recommended_cpus == 2f32);
    assert!(pod.min_memory == 4294967296);
    assert!(pod.gpu_spec_requirments == config.gpu_spec_requirments);
}
