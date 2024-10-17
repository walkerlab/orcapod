use std::{collections::BTreeMap, fs, path::PathBuf};

use orcapod::{
    model::{Annotation, Pod, StreamInfo},
    store::{filestore::LocalFileStore, Store},
};

static TEST_DATA_STORAGE_FOLDER_NAME: &str = "orca-data-test";

struct TestEnviorment {}

/// Base on the style transfer use case with 2 inputs and 1 output
/// In reality doesn't really do much but add the two image together image + style
///
#[test]
fn test_pod() {
    let _env = TestEnviorment {};
    let store = LocalFileStore {
        directory: TEST_DATA_STORAGE_FOLDER_NAME.into(),
    };

    let mut input_stream_map = BTreeMap::<String, StreamInfo>::new();

    input_stream_map.insert(
        "input_1".into(),
        StreamInfo {
            path: "/image.png".into(),
            match_pattern: "*.png".into(),
        },
    );

    input_stream_map.insert(
        "input_2".into(),
        StreamInfo {
            path: "/style.png".into(),
            match_pattern: "*.png".into(),
        },
    );

    let mut output_stream_map = BTreeMap::<String, StreamInfo>::new();

    output_stream_map.insert(
        "final_img".into(),
        StreamInfo {
            path: "image.png".into(),
            match_pattern: "*.png".into(),
        },
    );

    let output_dir = PathBuf::from("/output");

    // Create the pod
    let pod = Pod::new(
        Annotation {
            name: "Style Transfer Test Pod".into(),
            version: "0.0.0".into(),
            description: "Part of orca test suite".into(),
        },
        "2995917a98ed7ca1d8b9d8e842c4560b8dd8415d".into(),
        "hello-world:latest".into(),
        "echo hello".into(),
        input_stream_map,
        output_dir,
        output_stream_map,
        2f32,
        4 << 10,
        None,
    )
    .unwrap();

    // Store the pod and test all functions
    assert!(store.save_pod(&pod).is_ok());
    assert!(store.list_pod().unwrap().len() == 1);
    assert!(
        pod == store
            .load_pod(&pod.annotation.name, &pod.annotation.version)
            .unwrap()
    );
    match store.delete_pod(&pod.annotation.name, &pod.annotation.version) {
        Ok(_) => (),
        Err(e) => panic!("{}", e),
    };
    assert!(store.list_pod().unwrap().len() == 0);
}

impl Drop for TestEnviorment {
    fn drop(&mut self) {
        match fs::remove_dir_all(TEST_DATA_STORAGE_FOLDER_NAME) {
            Ok(_) => (),
            Err(_) => (),
        }
    }
}
