#![expect(missing_docs, clippy::panic_in_result_fn, reason = "OK in tests.")]

pub mod fixture;
use fixture::{NAMESPACE_LOOKUP_READ_ONLY, pod_job_style, pod_result_style, pod_style};
use indoc::indoc;
use orcapod::{core::model::to_yaml, uniffi::error::Result};

#[test]
fn hash_pod() -> Result<()> {
    assert_eq!(
        pod_style()?.hash,
        "7cc9db247fdbe214520140ef610fc6c23a1f1c5a56e0a6868c72ead03f0be968",
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
            input_stream:
              base-input:
                path: /input
                match_pattern: input/.*
              extra-style:
                path: /extra_styles/style2.t7
                match_pattern: .*\.t7
            output_dir: /output
            output_stream:
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
        pod_job_style(&NAMESPACE_LOOKUP_READ_ONLY)?.hash,
        "b1b332a66917a7f561206ff8359a65066874727c16c57fe9a3b98d95eac7975b",
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
            pod: 7cc9db247fdbe214520140ef610fc6c23a1f1c5a56e0a6868c72ead03f0be968
            input_stream:
              base-input:
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
        "3689472bb271c10e9d4a3e15af3a04d63021a28d1349c2e77d1324ffde192a94",
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
            pod_job: b1b332a66917a7f561206ff8359a65066874727c16c57fe9a3b98d95eac7975b
            assigned_name: simple-endeavour
            status: Completed
            created: 1737922307
            terminated: 1737925907
            logs: Example logs
        "},
        "YAML serialization didn't match."
    );
    Ok(())
}
