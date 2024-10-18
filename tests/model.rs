#![expect(clippy::panic_in_result_fn, reason = "Panics OK in tests.")]

use std::error::Error;
pub mod fixture;
use fixture::pod_style;
use indoc::indoc;
use orcapod::model::{to_yaml, Pod};

#[test]
fn verify_hash() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        pod_style()?.hash,
        "C0F16323C742E1F06B82AA5EA74730DE6F879DE56B3110F681B17A61167ACCE2"
    );
    Ok(())
}

#[test]
fn verify_pod_to_yaml() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        to_yaml::<Pod>(&pod_style()?)?,
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
            min_cpus: 0.25
            min_memory: 2147483648
            output_dir: /output
            output_stream_map:
              styled:
                path: ./styled.png
                match_pattern: ./styled.png
            required_gpu: null
            source_commit: https://github.com/zenml-io/zenml/tree/0.67.0
        "}
    );
    Ok(())
}
