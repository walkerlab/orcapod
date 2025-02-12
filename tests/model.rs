#![expect(clippy::panic_in_result_fn, reason = "Panics OK in tests.")]

pub mod fixture;
use fixture::{pod_job_style, pod_result_style, pod_style, store_fixture, FakeStore};
use indoc::indoc;
use orcapod::{error::Result, model::to_yaml};

#[test]
fn hash_pod() -> Result<()> {
    assert_eq!(
        pod_style()?.hash,
        "8e34979d6c526e5948bafbfa42e3df23c42b2a98082b97510f64f77b8a68e094",
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
            image: example.server.com/user/style-transfer:1.0.0
            command: python /run.py
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
            source_commit_url: https://github.com/user/style-transfer/tree/1.0.0
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
        pod_job.hash, "ff0f48e9edf63a5170f78b6fc9e6231cbdf2fae9f2665753992bb1a7bd0f60a6",
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
              style:
                kind: File
                location: styles/mosaic.t7
                checksum: null
            output_stream_mapping: output
            cpu_limit: 0.5
            memory_limit: 2147483648
            env_vars: null
        "},
        "YAML serialization didn't match."
    );
    Ok(())
}

#[test]
fn hash_pod_result() -> Result<()> {
    assert_eq!(
        pod_result_style(&FakeStore)?.hash,
        "53e418976509b75b9fae778bde148d5f0e42567caa43f78b39b329d9d8409c62",
        "Hash didn't match."
    );
    Ok(())
}

#[test]
fn pod_result_to_yaml() -> Result<()> {
    assert_eq!(
        to_yaml(&pod_result_style(&FakeStore)?)?,
        indoc! {"
            class: pod_result
            pod_job: 38f2021f67a8be0498ff1092789670661521e11c317d92ccebbc9dfeda98df7f
            assigned_name: simple-endeavour
            status: Completed
            created: 1737922307
            terminated: 1737925907
        "},
        "YAML serialization didn't match."
    );
    Ok(())
}
