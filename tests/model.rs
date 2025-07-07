#![expect(missing_docs, clippy::panic_in_result_fn, reason = "OK in tests.")]

pub mod fixture;
use fixture::{NAMESPACE_LOOKUP_READ_ONLY, pod_job_style, pod_result_style, pod_style};
use indoc::indoc;
use orcapod::{core::model::to_yaml, uniffi::error::Result};

#[test]
fn hash_pod() -> Result<()> {
    assert_eq!(
        pod_style()?.hash,
        "0e993f645fbb36f0635e2c9140975997cf4ca723d0b49cf4ee4963b76e6424d7",
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
            command:
            - python
            - /run.py
            input_spec:
              base-input:
                path: /input
                match_pattern: input/.*
              extra-style:
                path: /extra_styles/style2.t7
                match_pattern: .*\.t7
            output_dir: /output
            output_spec:
              result1:
                path: result1.jpeg
                match_pattern: .*\.jpeg
              result2:
                path: result2.jpeg
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
        pod_job_style(&NAMESPACE_LOOKUP_READ_ONLY)?.hash,
        "e94bd190b35ff10cf0f3d808fe45101564860c0a701d60baf8528401e1206cf6",
        "Hash didn't match."
    );
    Ok(())
}

#[test]
fn pod_job_to_yaml() -> Result<()> {
    assert_eq!(
        to_yaml(&pod_job_style(&NAMESPACE_LOOKUP_READ_ONLY)?)?,
        indoc! {"
            class: pod_job
            pod: 0e993f645fbb36f0635e2c9140975997cf4ca723d0b49cf4ee4963b76e6424d7
            input_packet:
              base-input:
                blobs:
                - kind: File
                  location:
                    namespace: default
                    path: styles/style1.t7
                  checksum: 69e709c1697e290994d2da75ddfb2097bf801a9436a3727a282e0230e703da2b
                - kind: File
                  location:
                    namespace: default
                    path: images/subject.jpeg
                  checksum: 8b44b8ea83b1f5eec3ac16cf941767e629896c465803fb69c21adbbf984516bd
              extra-style:
                blob:
                  kind: File
                  location:
                    namespace: default
                    path: styles/mosaic.t7
                  checksum: fbd7d882e9e02aafb57366e726762025ff6b2e12cd41abd44b874542b7693771
            output_dir:
              namespace: default
              path: output
            cpu_limit: 0.5
            memory_limit: 2147483648
            env_vars:
              AAA: SORT
              ZZZ: PLEASE
        "},
        "YAML serialization didn't match."
    );
    Ok(())
}

#[test]
fn hash_pod_result() -> Result<()> {
    assert_eq!(
        pod_result_style(&NAMESPACE_LOOKUP_READ_ONLY)?.hash,
        "12a482e8672fd5617b76c64408c2303c83af40bec5f7b06763e7ba24de4ae2e0",
        "Hash didn't match."
    );
    Ok(())
}

#[test]
fn pod_result_to_yaml() -> Result<()> {
    assert_eq!(
        to_yaml(&pod_result_style(&NAMESPACE_LOOKUP_READ_ONLY)?)?,
        indoc! {"
            class: pod_result
            pod_job: e94bd190b35ff10cf0f3d808fe45101564860c0a701d60baf8528401e1206cf6
            output_packet:
              result1:
                blob:
                  kind: File
                  location:
                    namespace: default
                    path: output/result1.jpeg
                  checksum: 5898ca5bed67147680c6489056cbf2e90074bc51d8ca2645453742580ce74b7a
              result2:
                blob:
                  kind: File
                  location:
                    namespace: default
                    path: output/result2.jpeg
                  checksum: da71a1b5f8ca6ebd1edfd11df4c83078fc50d0e6a4c9b3d642ba397d81d8e883
            assigned_name: simple-endeavour
            status: Completed
            created: 1737922307
            terminated: 1737925907
        "},
        "YAML serialization didn't match."
    );
    Ok(())
}
