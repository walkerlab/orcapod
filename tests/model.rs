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
        "c18841b375242ca10b166c11c9cb23f41aa9292e7f9115b290f759a44fd7db99",
        "Hash didn't match."
    );
    Ok(())
}

#[test]
fn pod_to_yaml() -> Result<()> {
    assert_eq!(
        to_yaml::<Pod>(&pod_style()?)?,
        indoc! {"
            class: pod
            image: zenmldocker/zenml-server:0.67.0
            command: tail -f /dev/null
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
            source_commit_url: https://github.com/zenml-io/zenml/tree/0.67.0
            recommended_cpus: 0.25
            recommended_memory: 2147483648
            required_gpu: null
        "},
        "YAML serialization didn't match."
    );
    Ok(())
}
