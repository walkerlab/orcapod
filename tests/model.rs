#![expect(clippy::panic_in_result_fn, reason = "Panics OK in tests.")]

pub mod fixture;
use fixture::{pod_job_style, pod_result_style, pod_style, FakeStore};
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
    assert_eq!(
        pod_job_style(&FakeStore)?.hash,
        "38f2021f67a8be0498ff1092789670661521e11c317d92ccebbc9dfeda98df7f",
        "Hash didn't match."
    );
    Ok(())
}

#[test]
fn pod_job_to_yaml() -> Result<()> {
    assert_eq!(
        to_yaml(&pod_job_style(&FakeStore)?)?,
        indoc! {"
            class: pod_job
            pod: 8e34979d6c526e5948bafbfa42e3df23c42b2a98082b97510f64f77b8a68e094
            input_stream_path:
              image:
                kind: File
                location: images/dog.jpeg
                checksum: fake_hash
              style:
                kind: File
                location: styles/mosaic.t7
                checksum: fake_hash
            output_stream_path:
              kind: Folder
              location: output
              checksum: null
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
        "511e6e1ce8fd8dc2a4775b6372c2779283fa93b379976012dd2a25dc88a1ae3a",
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
            state: Completed
            created: 1737922307
            terminated: 1737925907
        "},
        "YAML serialization didn't match."
    );
    Ok(())
}
