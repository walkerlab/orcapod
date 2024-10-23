#![expect(clippy::panic_in_result_fn, reason = "Panics OK in tests.")]

use std::error::Error;
pub mod fixture;
use fixture::get_test_pod;
use indoc::indoc;
use orcapod::model::{to_yaml, Pod};

#[test]
fn verify_hash() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        get_test_pod()?.hash,
        "13d69656d396c272588dd875b2802faee1a56bd985e3c43c7db276a373bc9ddb"
    );
    Ok(())
}

#[test]
fn verify_pod_to_yaml() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        to_yaml::<Pod>(&get_test_pod()?)?,
        indoc! {"
            class: pod
            command: tail -f /dev/null
            image: zenmldocker/zenml-server:0.67.0
            input_stream_map:
              image:
                path: /input/image.png
                match_pattern: /input/image.png
              painting:
                path: /input/painting.png
                match_pattern: /input/painting.png
            output_dir: /output
            output_stream_map:
              styled:
                path: ./styled.png
                match_pattern: ./styled.png
            recommended_cpus: 0.25
            recommended_memory: 2147483648
            required_gpu: null
            source_commit_url: https://github.com/zenml-io/zenml/tree/0.67.0
        "}
    );
    Ok(())
}
