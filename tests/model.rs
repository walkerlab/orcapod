#![expect(clippy::panic_in_result_fn, reason = "Panics OK in tests.")]

pub mod fixture;
use fixture::pod_style;
use indoc::indoc;
use orcapod::{
    error::Result,
    model::{to_yaml, Pod},
};

#[test]
fn hash() -> Result<()> {
    assert_eq!(
        pod_style()?.hash,
        "54ff593c1651ac21fb5c17846728eae39420056f505a8fdc3a32c995cc33ab67",
        "Hash didn't match."
    );
    Ok(())
}

#[test]
fn pod_to_yaml() -> Result<()> {
    assert_eq!(
        to_yaml::<Pod>(&pod_style()?)?,
        indoc! {r"
            class: pod
            image: zenmldocker/zenml-server:0.67.0
            command: tail -f /dev/null
            input_stream_map:
              image:
                path: /input/image.png
                match_pattern: .*\.png
              style:
                path: /input/style.png
                match_pattern: .*\.png
            output_dir: /output
            output_stream_map:
              result:
                path: ./result.png
                match_pattern: .*\.png
            source_commit_url: https://github.com/zenml-io/zenml/tree/0.67.0
            recommended_cpus: 0.25
            recommended_memory: 2147483648
            required_gpu: null
        "},
        "YAML serialization didn't match."
    );
    Ok(())
}
