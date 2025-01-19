#![expect(clippy::panic_in_result_fn, reason = "Panics OK in tests.")]

pub mod fixture;
use anyhow::Result;
use fixture::{pod_job_style, pod_style};
use indoc::indoc;
use orcapod::model::to_yaml;

#[test]
fn hash_pod() -> Result<()> {
    assert_eq!(
        pod_style()?.hash,
        "1f20028a4981041172dbb808000a2c42ae6e2997b8b7894970763f74df15f5e8",
        "Hash didn't match."
    );
    Ok(())
}

#[test]
fn pod_to_yaml() -> Result<()> {
    assert_eq!(
        to_yaml(&pod_style()?)?,
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
            recommended_memory: 1073741824
            required_gpu: null
        "},
        "YAML serialization didn't match."
    );
    Ok(())
}

#[test]
fn hash_pod_job() -> Result<()> {
    assert_eq!(
        pod_job_style()?.hash,
        "94f0e649aa3790ab9914c0e3b895e1a33fbfbf3009839b60c826355eb8088f71",
        "Hash didn't match."
    );
    Ok(())
}

#[test]
fn pod_job_to_yaml() -> Result<()> {
    assert_eq!(
        to_yaml(&pod_job_style()?)?,
        indoc! {"
            class: pod_job
            pod: 1f20028a4981041172dbb808000a2c42ae6e2997b8b7894970763f74df15f5e8
            cpu_limit: 0.5
            memory_limit: 2147483648
        "},
        "YAML serialization didn't match."
    );
    Ok(())
}
