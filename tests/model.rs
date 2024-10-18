#![expect(clippy::panic_in_result_fn, reason = "Panics OK in tests.")]

use core::error::Error;
pub mod fixture;
use fixture::pod_style;
use indoc::indoc;
use orcapod::model::{to_yaml, Pod};

#[test]
fn verify_hash() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        pod_style()?.hash,
        "4A7CA5CEA3BC814B73ED0F5695F5AFB40E7762D9133BFCC9800A8A37FC8BBB96"
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
            minimum_cpus: 0.25
            minimum_memory: 2147483648
            output_dir: /output
            output_stream_map:
              styled:
                path: ./styled.png
                match_pattern: ./styled.png
            required_gpu: null
            source_commit_url: https://github.com/zenml-io/zenml/tree/0.67.0
        "}
    );
    Ok(())
}
