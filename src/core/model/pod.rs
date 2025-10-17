use crate::uniffi::model::pod::{Pod, PodJob};
use serde::{Deserialize as _, Deserializer};
use serde_yaml::{self, Value};
use std::{result, sync::Arc};

#[expect(clippy::expect_used, reason = "Serde requires this signature.")]
pub fn deserialize_pod<'de, D>(deserializer: D) -> result::Result<Arc<Pod>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    (value).as_str().map_or_else(
        || {
            Ok(serde_yaml::from_value(value.clone())
                .expect("Failed to convert from serde value to specific type."))
        },
        |hash| {
            Ok({
                Pod {
                    hash: hash.to_owned(),
                    ..Pod::default()
                }
                .into()
            })
        },
    )
}

#[expect(clippy::expect_used, reason = "Serde requires this signature.")]
pub fn deserialize_pod_job<'de, D>(deserializer: D) -> result::Result<Arc<PodJob>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    (value).as_str().map_or_else(
        || {
            Ok(serde_yaml::from_value(value.clone())
                .expect("Failed to convert from serde value to specific type."))
        },
        |hash| {
            Ok({
                PodJob {
                    hash: hash.to_owned(),
                    ..PodJob::default()
                }
                .into()
            })
        },
    )
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "OK in tests.")]
    use indoc::indoc;
    use std::sync::{Arc, LazyLock};
    use std::{collections::HashMap, path::PathBuf};

    use crate::core::model::ToYaml as _;
    use crate::uniffi::model::packet::{Blob, BlobKind, PathSet, URI};
    use crate::uniffi::model::pod::PodResult;
    use crate::uniffi::orchestrator::PodStatus;
    use crate::uniffi::{
        error::Result,
        model::{
            Annotation,
            packet::PathInfo,
            pod::{Pod, PodJob, RecommendSpecs},
        },
    };

    use pretty_assertions::assert_eq;

    static TEST_FILE_NAMESPACE_LOOKUP: LazyLock<HashMap<String, PathBuf>> = LazyLock::new(|| {
        HashMap::from([
            ("input".into(), PathBuf::from("tests/extra/data/input_txt")),
            ("output".into(), PathBuf::from("tests/extra/data/output")),
        ])
    });

    fn basic_pod() -> Result<Pod> {
        Pod::new(
            Some(Annotation {
                name: "test".into(),
                version: "0.1".into(),
                description: "Basic pod for testing hashing and yaml serialization".into(),
            }),
            "alpine:3.14".into(),
            vec!["cp", "/input/input.txt", "/output/output.txt"]
                .into_iter()
                .map(String::from)
                .collect(),
            HashMap::from([(
                "input_txt".into(),
                PathInfo {
                    path: "/input/input.txt".into(),
                    match_pattern: r".*\.txt".into(),
                },
            )]),
            "/output".into(),
            HashMap::from([(
                "output_txt".into(),
                PathInfo {
                    path: "output.txt".into(),
                    match_pattern: r".*\.txt".into(),
                },
            )]),
            RecommendSpecs {
                cpus: 0.20,
                memory: 128 << 20,
            },
            None,
        )
    }

    fn basic_pod_job() -> Result<PodJob> {
        let pod = Arc::new(basic_pod()?);
        PodJob::new(
            Some(Annotation {
                name: "test_job".into(),
                version: "0.1".into(),
                description: "Basic pod job for testing hashing and yaml serialization".into(),
            }),
            Arc::clone(&pod),
            HashMap::from([(
                "input_txt".into(),
                PathSet::Unary(Blob::new(
                    BlobKind::File,
                    URI {
                        namespace: "input".into(),
                        path: "cat.txt".into(),
                    },
                )),
            )]),
            URI {
                namespace: "output".into(),
                path: "".into(),
            },
            pod.recommend_specs.cpus,
            pod.recommend_specs.memory,
            Some(HashMap::from([("FAKE_ENV".into(), "FakeValue".into())])),
            &TEST_FILE_NAMESPACE_LOOKUP,
        )
    }

    fn basic_pod_result() -> Result<PodResult> {
        PodResult::new(
            Some(Annotation {
                name: "test".into(),
                version: "0.1".into(),
                description: "Basic Result for testing hashing and yaml serialization".into(),
            }),
            basic_pod_job()?.into(),
            "randomly_assigned_name".into(),
            PodStatus::Completed,
            1_737_922_307,
            1_737_925_907,
            &TEST_FILE_NAMESPACE_LOOKUP,
            "example_logs".to_owned(),
        )
    }

    #[test]
    fn pod_hash() {
        assert_eq!(
            basic_pod().unwrap().hash,
            "b5574e2efdf26361e8e8e886389a250cfbfcceed08b29325a78fd738cbb2a1b8",
            "Hash didn't match."
        );
    }

    #[test]
    fn pod_to_yaml() {
        assert_eq!(
            basic_pod().unwrap().to_yaml().unwrap(),
            indoc! {r"
                class: pod
                image: alpine:3.14
                command:
                - cp
                - /input/input.txt
                - /output/output.txt
                input_spec:
                  input_txt:
                    path: /input/input.txt
                    match_pattern: .*\.txt
                output_dir: /output
                output_spec:
                  output_txt:
                    path: output.txt
                    match_pattern: .*\.txt
                gpu_requirements: null
            "},
            "YAML serialization didn't match."
        );
    }

    #[test]
    fn pod_job_hash() {
        assert_eq!(
            basic_pod_job().unwrap().hash,
            "80348a4ef866a9dfc1a5d0a48467a6592ef2ed9e8de67930d64afefbb395f1c6",
            "Hash didn't match."
        );
    }

    #[test]
    fn pod_job_to_yaml() {
        assert_eq!(
            basic_pod_job().unwrap().to_yaml().unwrap(),
            indoc! {"
                class: pod_job
                pod: b5574e2efdf26361e8e8e886389a250cfbfcceed08b29325a78fd738cbb2a1b8
                input_packet:
                  input_txt:
                    kind: File
                    location:
                      namespace: input
                      path: cat.txt
                    checksum: 175cc6f362b2f75acd08a373e000144fdb8d14a833d4b70fd743f16a7039103f
                output_dir:
                  namespace: output
                  path: ''
                cpu_limit: 0.2
                memory_limit: 134217728
                env_vars:
                  FAKE_ENV: FakeValue
            "},
            "YAML serialization didn't match."
        );
    }

    #[test]
    fn pod_result_hash() {
        assert_eq!(
            basic_pod_result().unwrap().hash,
            "92809a4ce13b4fe8c8dcdcf2b48dd14a9dd885593fe3ab5d9809d27bc9a16354",
            "Hash didn't match."
        );
    }

    #[test]
    fn pod_result_to_yaml() {
        assert_eq!(
            basic_pod_result().unwrap().to_yaml().unwrap(),
            indoc! {"
                class: pod_result
                pod_job: 80348a4ef866a9dfc1a5d0a48467a6592ef2ed9e8de67930d64afefbb395f1c6
                output_packet:
                  output_txt:
                    kind: File
                    location:
                      namespace: output
                      path: output.txt
                    checksum: 175cc6f362b2f75acd08a373e000144fdb8d14a833d4b70fd743f16a7039103f
                assigned_name: randomly_assigned_name
                status: Completed
                created: 1737922307
                terminated: 1737925907
                logs: example_logs
            "},
            "YAML serialization didn't match."
        );
    }
}
