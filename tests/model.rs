#![expect(missing_docs, reason = "OK in tests.")]

pub mod fixture;
use fixture::{NAMESPACE_LOOKUP_READ_ONLY, pod_job_style, pod_result_style, pod_style};
use indoc::indoc;
use orcapod::{core::model::ToYaml as _, uniffi::error::Result};
use pretty_assertions::assert_eq as pretty_assert_eq;

#[test]
fn hash_pod() -> Result<()> {
    pretty_assert_eq!(
        pod_style()?.hash,
        "3ee06e06cc1c821712a2a8fd98a6aa3e72ab0e78d5a2668d39b3400d5ece5d52",
        "Hash didn't match."
    );
    Ok(())
}

#[test]
fn pod_to_yaml() -> Result<()> {
    pretty_assert_eq!(
        pod_style()?.to_yaml()?,
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
        "},
        "YAML serialization didn't match."
    );
    Ok(())
}

#[test]
fn hash_pod_job() -> Result<()> {
    pretty_assert_eq!(
        pod_job_style(&NAMESPACE_LOOKUP_READ_ONLY)?.hash,
        "0bc729c0d718f01d35d42856f5303353b95b280e441410d623372e0d29e9f1c9",
        "Hash didn't match."
    );
    Ok(())
}

#[test]
fn pod_job_to_yaml() -> Result<()> {
    pretty_assert_eq!(
        pod_job_style(&NAMESPACE_LOOKUP_READ_ONLY)?.to_yaml()?,
        indoc! {"
            class: pod_job
            pod: 3ee06e06cc1c821712a2a8fd98a6aa3e72ab0e78d5a2668d39b3400d5ece5d52
            input_packet:
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
    pretty_assert_eq!(
        pod_result_style(&NAMESPACE_LOOKUP_READ_ONLY)?.hash,
        "72a6586ccb1af6892c14c608757dbd43b7f8092ffeaee57ceb21d6bbffe6a92f",
        "Hash didn't match."
    );
    Ok(())
}

#[test]
fn pod_result_to_yaml() -> Result<()> {
    pretty_assert_eq!(
        pod_result_style(&NAMESPACE_LOOKUP_READ_ONLY)?.to_yaml()?,
        indoc! {"
            class: pod_result
            pod_job: 0bc729c0d718f01d35d42856f5303353b95b280e441410d623372e0d29e9f1c9
            output_packet:
              result1:
                kind: File
                location:
                  namespace: default
                  path: output/result1.jpeg
                checksum: 5898ca5bed67147680c6489056cbf2e90074bc51d8ca2645453742580ce74b7a
              result2:
                kind: File
                location:
                  namespace: default
                  path: output/result2.jpeg
                checksum: a1458fc7d7d9d23a66feae88b5a89f1756055bdbb6be02fdf672f7d31ed92735
            assigned_name: simple-endeavour
            status: Completed
            created: 1737922307
            terminated: 1737925907
        "},
        "YAML serialization didn't match."
    );
    Ok(())
}
