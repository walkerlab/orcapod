#![expect(clippy::panic_in_result_fn, reason = "Panics OK in tests.")]

pub mod fixture;
use fixture::pod_style;
use indoc::indoc;
use orcapod::{
    error::OrcaResult,
    model::{to_yaml, Pod},
};

#[test]
fn verify_hash() -> OrcaResult<()> {
    assert_eq!(
        pod_style()?.hash,
        "13D69656D396C272588DD875B2802FAEE1A56BD985E3C43C7DB276A373BC9DDB"
    );
    Ok(())
}

#[test]
fn verify_pod_to_yaml() -> OrcaResult<()> {
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
