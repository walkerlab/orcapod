#![expect(clippy::panic_in_result_fn, reason = "Panics OK in tests.")]

pub mod fixture;
use fixture::{pod_job_style, pod_style};
use indoc::indoc;
use orcapod::{
    error::Result,
    model::{to_yaml, Blob, BlobInterface, FileOrFolder},
};

struct FakeStore;
impl BlobInterface for FakeStore {
    fn compute_checksum(&self, blob: Blob<FileOrFolder>) -> Result<Blob<FileOrFolder>> {
        Ok(Blob {
            checksum: Some("fake_hash".to_owned()),
            ..blob
        })
    }
}

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
    assert_eq!(
        pod_job_style(&FakeStore)?.hash,
        "5851a796f77e1649aa9b1e9704dd958f04af16e032bfa29d781db80d0e3ad243",
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
            pod: 61d893c39c059b3f6d5e6490edbff1ec2118404ace5031f0b1f5da8a06861085
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
        "},
        "YAML serialization didn't match."
    );
    Ok(())
}
