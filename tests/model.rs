#![expect(clippy::panic_in_result_fn, reason = "Panics OK in tests.")]

pub mod fixture;
use fixture::{pod_job_style, pod_style, store_fixture};
use indoc::indoc;
use orcapod::{error::Result, model::to_yaml};

#[test]
fn hash_pod() -> Result<()> {
    assert_eq!(
        pod_style()?.hash,
        "61d893c39c059b3f6d5e6490edbff1ec2118404ace5031f0b1f5da8a06861085",
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
                path: /input/image.jpeg
                match_pattern: .*\.jpeg
              style:
                path: /input/style.t7
                match_pattern: .*\.t7
            output_dir: /output
            output_stream_map:
              result:
                path: ./result.jpeg
                match_pattern: .*\.jpeg
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
    let mut pod_job = pod_job_style()?;
    pod_job.compute_checksum_for_input_stream_path(&store_fixture(None, true)?.store)?;
    assert_eq!(
        pod_job.hash, "aa83d37781f010a4a0833c9d3e96a58ef3ab7956fc9cedb43b67724a6c578269",
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
            pod: 61d893c39c059b3f6d5e6490edbff1ec2118404ace5031f0b1f5da8a06861085
            input_stream_mapping:
              image:
                kind: File
                location: images/dog.jpeg
                checksum: null
                store_pointer_name: null
              style:
                kind: File
                location: styles/mosaic.t7
                checksum: null
                store_pointer_name: null
            output_stream_mapping: output
            cpu_limit: 0.5
            memory_limit: 2147483648
            retry_policy: NoRetry
        "},
        "YAML serialization didn't match."
    );
    Ok(())
}
