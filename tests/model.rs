use std::collections::BTreeMap;

use orcapod::{
    model::KeyInfo,
    orca::{create_pod, StorageBackend},
};

static TEST_DATA_STORAGE_FOLDER_NAME: &str = "orca-data-test";

/// Base on the style transfer use case with 2 inputs and 1 output
/// In reality doesn't really do much but add the two image together image + style
#[test]
fn test_pod() {
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

    let output_dir = "/output".to_string();

    let result = create_pod(
        "test_style_transfer_pod".into(),
        "Redraw image base on the style img".into(),
        "1.0.0".into(),
        input_stream_map,
        output_dir.into(),
        output_stream_map,
        "FAKE DOCKER YAML".into(),
        "FAKE Source hash".into(),
        None,
        None,
        None,
        StorageBackend::FileStore(TEST_DATA_STORAGE_FOLDER_NAME.into()),
    );

    assert!(result.is_ok());
}
