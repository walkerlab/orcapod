use std::{backtrace::Backtrace, collections::HashMap, path::PathBuf, result, sync::Arc};

use derive_more::Display;
use getset::CloneGetters;
use serde::{Deserialize, Deserializer, Serialize};
use serde_yaml::Value;

use crate::{
    crypto::{hash_blob, hash_buffer},
    error::{Kind, OrcaError, Result},
    model::{
        Annotation, ToYaml,
        packet::{Blob, BlobKind, Packet, PathInfo, PathSet, URI},
        serialize_hashmap, serialize_hashmap_option,
    },
    util::get,
    validation::validate_packet,
};

/// A reusable, containerized computational unit.
#[derive(
    uniffi::Object, Serialize, Deserialize, Debug, PartialEq, Default, Clone, Display, CloneGetters,
)]
#[getset(get_clone, impl_attrs = "#[uniffi::export]")]
#[display("{self:#?}")]
#[uniffi::export(Display)]
pub struct Pod {
    /// Metadata that doesn't affect reproducibility.
    pub annotation: Option<Annotation>,
    /// Unique id based on reproducibility.
    #[serde(default)]
    pub hash: String,
    /// Reproducible environment for compute.
    pub image: String,
    /// Shell command to begin computation. First element is the executable and remaining elements
    /// are the arguments.
    pub command: Vec<String>,
    /// Exposed, internal input specification.
    #[serde(serialize_with = "serialize_hashmap")]
    pub input_spec: HashMap<String, PathInfo>,
    /// Exposed, internal output directory.
    pub output_dir: PathBuf,
    /// Exposed, internal output specification.
    #[serde(serialize_with = "serialize_hashmap")]
    pub output_spec: HashMap<String, PathInfo>,
    /// Execution requirements for the pod.
    #[serde(default)]
    pub recommend_specs: RecommendedSpecs,
    /// Optional GPU requirements for the pod. If set, then the running system needs a GPU that meets the requirements.
    pub gpu_requirements: Option<GPURequirement>,
}

#[uniffi::export]
impl Pod {
    /// Construct a new pod instance.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue initializing a `Pod` instance.
    #[uniffi::constructor]
    pub fn new(
        annotation: Option<Annotation>,
        image: String,
        command: Vec<String>,
        input_spec: HashMap<String, PathInfo>,
        output_dir: PathBuf,
        output_spec: HashMap<String, PathInfo>,
        recommend_specs: RecommendedSpecs,
        gpu_requirements: Option<GPURequirement>,
    ) -> Result<Self> {
        let pod_no_hash = Self {
            annotation,
            hash: String::new(),
            image,
            command,
            input_spec,
            output_dir,
            output_spec,
            recommend_specs,
            gpu_requirements,
        };
        Ok(Self {
            hash: hash_buffer(pod_no_hash.to_yaml()?),
            ..pod_no_hash
        })
    }
}

impl ToYaml for Pod {
    fn process_field(
        field_name: &str,
        field_value: &serde_yaml::Value,
    ) -> Option<(String, serde_yaml::Value)> {
        match field_name {
            "annotation" | "hash" | "recommend_specs" => None,
            _ => Some((field_name.to_owned(), field_value.clone())),
        }
    }
}

/// Execution recommendations for a pod, since it doesn't impact the actual reproducibility
/// it shouldn't be hashed along with the pod
#[derive(uniffi::Record, Serialize, Deserialize, Debug, PartialEq, Default, Clone)]
pub struct RecommendedSpecs {
    /// Optimal number of CPU cores needed to run the pod provided by the user
    pub cpus: f32,
    /// Optimal amount of memory needed to run the pod provided by the user, code can probably run with less but may hit OOM
    pub memory: u64,
}

impl ToYaml for RecommendedSpecs {
    fn process_field(
        field_name: &str,
        field_value: &serde_yaml::Value,
    ) -> Option<(String, serde_yaml::Value)> {
        Some((field_name.to_owned(), field_value.clone()))
    }
}

/// Specification for GPU requirements in computation.
#[derive(uniffi::Record, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct GPURequirement {
    /// GPU model specification.
    pub model: GPUModel,
    /// Manufacturer recommended memory.
    pub recommended_memory: u64,
    /// Number of GPU cards required.
    pub count: u16,
}

/// GPU model specification.
#[derive(uniffi::Enum, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum GPUModel {
    /// NVIDIA-manufactured card where `String` is the specific minimum CUDA version in X.XX
    NVIDIA(String),
    /// Any GPU architecture, code is generic enough
    Any,
}

/// A compute job that specifies resource requests and input/output targets.
///
/// `PodJob` represents a specific execution instance of a [`Pod`] with concrete
/// input data, resource limits, and output specifications. It includes all the
/// information needed to run a containerized computation job.
#[derive(
    uniffi::Object, Serialize, Deserialize, Debug, PartialEq, Clone, Default, Display, CloneGetters,
)]
#[getset(get_clone, impl_attrs = "#[uniffi::export]")]
#[display("{self:#?}")]
#[uniffi::export(Display)]
pub struct PodJob {
    /// Metadata that doesn't affect reproducibility.
    pub annotation: Option<Annotation>,
    /// Unique id based on reproducibility.
    #[serde(default)]
    pub hash: String,
    /// A pod to base the pod job on.
    #[serde(deserialize_with = "deserialize_pod")]
    pub pod: Arc<Pod>,
    /// Attached, external input packet.
    #[serde(serialize_with = "serialize_hashmap")]
    pub input_packet: Packet,
    /// Attached, external output directory.
    pub output_dir: URI,
    /// Maximum allowable cores in fractional cores for the computation.
    pub cpu_limit: f32,
    /// Maximum allowable memory in bytes for the computation.
    pub memory_limit: u64,
    /// Environment variables to be set in environment.
    #[serde(serialize_with = "serialize_hashmap_option")]
    pub env_vars: Option<HashMap<String, String>>,
}

#[uniffi::export]
impl PodJob {
    /// Construct a new pod job instance.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue initializing a `PodJob` instance.
    #[uniffi::constructor]
    pub fn new(
        annotation: Option<Annotation>,
        pod: Arc<Pod>,
        mut input_packet: Packet,
        output_dir: URI,
        cpu_limit: f32,
        memory_limit: u64,
        env_vars: Option<HashMap<String, String>>,
        namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<Self> {
        validate_packet("input".into(), &pod.input_spec, &input_packet)?;
        input_packet = input_packet
            .iter()
            .map(|(stream_name, stream_input)| match stream_input {
                PathSet::Unary(blob) => Ok((
                    stream_name.clone(),
                    PathSet::Unary(hash_blob(namespace_lookup, blob)?),
                )),
                PathSet::Collection(blobs) => Ok((
                    stream_name.clone(),
                    PathSet::Collection(
                        blobs
                            .iter()
                            .map(|blob| hash_blob(namespace_lookup, blob))
                            .collect::<Result<Vec<_>>>()?,
                    ),
                )),
            })
            .collect::<Result<_>>()?;
        let pod_job_no_hash = Self {
            annotation,
            hash: String::new(),
            pod,
            input_packet,
            output_dir,
            cpu_limit,
            memory_limit,
            env_vars,
        };
        Ok(Self {
            hash: hash_buffer(pod_job_no_hash.to_yaml()?),
            ..pod_job_no_hash
        })
    }
}

impl ToYaml for PodJob {
    fn process_field(
        field_name: &str,
        field_value: &serde_yaml::Value,
    ) -> Option<(String, serde_yaml::Value)> {
        match field_name {
            "annotation" | "hash" => None,
            "pod" => Some((field_name.to_owned(), field_value["hash"].clone())),
            _ => Some((field_name.to_owned(), field_value.clone())),
        }
    }
}

/// Result from a compute job run.
#[derive(uniffi::Record, Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct PodResult {
    /// Metadata that doesn't affect reproducibility.
    pub annotation: Option<Annotation>,
    /// Unique id based on reproducibility.
    #[serde(default)]
    pub hash: String,
    /// A pod job that originated the pod result.
    #[serde(deserialize_with = "deserialize_pod_job")]
    pub pod_job: Arc<PodJob>,
    /// Produced, external output packet.
    #[serde(serialize_with = "serialize_hashmap")]
    pub output_packet: Packet,
    /// Name given by orchestrator.
    pub assigned_name: String,
    /// Status of compute run when terminated.
    pub status: PodStatus,
    /// Time in epoch when created in seconds.
    pub created: u64,
    /// Time in epoch when terminated in seconds.
    pub terminated: u64,
    /// Logs about stdout and stderr, where stderr is append at the end
    pub logs: String,
}

impl PodResult {
    /// Construct a new pod result instance.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue initializing a `PodResult` instance.
    pub fn new(
        annotation: Option<Annotation>,
        pod_job: Arc<PodJob>,
        assigned_name: String,
        status: PodStatus,
        created: u64,
        terminated: u64,
        namespace_lookup: &HashMap<String, PathBuf>,
        logs: String,
    ) -> Result<Self> {
        let output_packet = pod_job
            .pod
            .output_spec
            .iter()
            .filter_map(|(packet_key, path_info)| {
                let location = URI {
                    namespace: pod_job.output_dir.namespace.clone(),
                    path: pod_job.output_dir.path.join(&path_info.path),
                };

                let local_location = match get(namespace_lookup, &location.namespace) {
                    Ok(root_path) => root_path.join(&location.path),
                    Err(error) => return Some(Err(error)),
                };

                match local_location.try_exists() {
                    Ok(false) => None,
                    Err(error) => Some(Err(OrcaError {
                        kind: Kind::InvalidPath {
                            path: local_location.clone(),
                            source: error,
                            backtrace: Some(Backtrace::capture()),
                        },
                    })),
                    Ok(true) => Some(Ok((
                        packet_key,
                        Blob {
                            kind: if local_location.is_file() {
                                BlobKind::File
                            } else {
                                BlobKind::Directory
                            },
                            location,
                            checksum: String::new(),
                        },
                    ))),
                }
            })
            .map(|result| {
                let (packet_key, blob) = result?;
                Ok((
                    packet_key.clone(),
                    PathSet::Unary(hash_blob(namespace_lookup, &blob)?),
                ))
            })
            .collect::<Result<_>>()?;

        // If packet is completed, the output packet must meet the output spec
        if matches!(status, PodStatus::Completed) {
            validate_packet("output".into(), &pod_job.pod.output_spec, &output_packet)?;
        }

        let pod_result_no_hash = Self {
            annotation,
            hash: String::new(),
            pod_job,
            output_packet,
            assigned_name,
            status,
            created,
            terminated,
            logs,
        };
        Ok(Self {
            hash: hash_buffer(pod_result_no_hash.to_yaml()?),
            ..pod_result_no_hash
        })
    }
}

impl ToYaml for PodResult {
    fn process_field(
        field_name: &str,
        field_value: &serde_yaml::Value,
    ) -> Option<(String, serde_yaml::Value)> {
        match field_name {
            "annotation" | "hash" => None,
            "pod_job" => Some((field_name.to_owned(), field_value["hash"].clone())),
            _ => Some((field_name.to_owned(), field_value.clone())),
        }
    }
}

/// Status of a particular compute run.
#[derive(uniffi::Enum, Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Default)]
pub enum PodStatus {
    /// Run is ongoing.
    Running,
    /// Run has completed successfully.
    Completed,
    /// Run failed with the provided error code.
    Failed(i16),
    /// For other container states that are not listed.
    Undefined,
    /// No status set.
    #[default]
    Unset,
}

#[expect(clippy::expect_used, reason = "Serde requires this signature.")]
fn deserialize_pod<'de, D>(deserializer: D) -> result::Result<Arc<Pod>, D::Error>
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
fn deserialize_pod_job<'de, D>(deserializer: D) -> result::Result<Arc<PodJob>, D::Error>
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
pub(crate) mod tests {
    #![expect(clippy::unwrap_used, reason = "OK in tests.")]
    use std::{
        collections::HashMap,
        path::PathBuf,
        sync::{Arc, LazyLock},
    };

    use indoc::indoc;

    use crate::{
        error::Result,
        model::{
            Annotation, ToYaml as _,
            packet::{Blob, BlobKind, PathInfo, PathSet, URI},
            pod::{Pod, PodJob, PodResult, PodStatus, RecommendedSpecs},
        },
    };

    use pretty_assertions::assert_eq;

    static TEST_FILE_NAMESPACE_LOOKUP: LazyLock<HashMap<String, PathBuf>> = LazyLock::new(|| {
        HashMap::from([
            ("input".into(), PathBuf::from("tests/extra/data/input_txt")),
            ("output".into(), PathBuf::from("tests/extra/data/output")),
        ])
    });

    pub fn pod_fixture() -> Result<Pod> {
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
            RecommendedSpecs {
                cpus: 0.20,
                memory: 128 << 20,
            },
            None,
        )
    }

    fn pod_job_fixture() -> Result<PodJob> {
        let pod = Arc::new(pod_fixture()?);
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

    fn pod_result_fixture() -> Result<PodResult> {
        PodResult::new(
            Some(Annotation {
                name: "test".into(),
                version: "0.1".into(),
                description: "Basic Result for testing hashing and yaml serialization".into(),
            }),
            pod_job_fixture()?.into(),
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
            pod_fixture().unwrap().hash,
            "b5574e2efdf26361e8e8e886389a250cfbfcceed08b29325a78fd738cbb2a1b8",
            "Hash didn't match."
        );
    }

    #[test]
    fn pod_to_yaml() {
        assert_eq!(
            pod_fixture().unwrap().to_yaml().unwrap(),
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
            pod_job_fixture().unwrap().hash,
            "80348a4ef866a9dfc1a5d0a48467a6592ef2ed9e8de67930d64afefbb395f1c6",
            "Hash didn't match."
        );
    }

    #[test]
    fn pod_job_to_yaml() {
        assert_eq!(
            pod_job_fixture().unwrap().to_yaml().unwrap(),
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
            pod_result_fixture().unwrap().hash,
            "92809a4ce13b4fe8c8dcdcf2b48dd14a9dd885593fe3ab5d9809d27bc9a16354",
            "Hash didn't match."
        );
    }

    #[test]
    fn pod_result_to_yaml() {
        assert_eq!(
            pod_result_fixture().unwrap().to_yaml().unwrap(),
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
