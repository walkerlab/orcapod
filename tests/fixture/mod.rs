#![expect(
    clippy::expect_used,
    clippy::missing_errors_doc,
    missing_docs,
    clippy::missing_panics_doc,
    clippy::unwrap_in_result,
    clippy::too_many_lines,
    reason = "OK in tests."
)]

use indoc::{formatdoc, indoc};
use names::{Generator, Name};
use orcapod::uniffi::{
    error::Result,
    model::{
        Annotation, Blob, BlobKind, InputSpecURI, Kernel, OutputSpecURI, Packet, PathInfo, PathSet,
        Pipeline, PipelineJob, Pod, PodJob, PodResult, URI,
    },
    orchestrator::PodStatus,
    store::{ModelID, ModelInfo, Store},
};
use std::{
    collections::HashMap,
    fs::{self, File, remove_dir_all},
    hash::RandomState,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, LazyLock},
};
use tempfile::TempDir;

// --- fixtures ---

pub static NAMESPACE_LOOKUP_READ_ONLY: LazyLock<HashMap<String, PathBuf>> =
    LazyLock::new(|| HashMap::from([("default".to_owned(), PathBuf::from("./tests/extra/data"))]));

pub fn pod_style() -> Result<Pod> {
    Pod::new(
        "example.server.com/user/style-transfer:1.0.0".to_owned(),
        str_to_vec("python /run.py"),
        HashMap::from([
            (
                "extra-style".to_owned(),
                PathInfo {
                    path: PathBuf::from("/extra_styles/style2.t7"),
                    match_pattern: r".*\.t7".to_owned(),
                },
            ),
            (
                "base-input".to_owned(),
                PathInfo {
                    path: PathBuf::from("/input"),
                    match_pattern: "input/.*".to_owned(),
                },
            ),
        ]),
        PathBuf::from("/output"),
        HashMap::from([
            (
                "result1".to_owned(),
                PathInfo {
                    path: PathBuf::from("result1.jpeg"),
                    match_pattern: r".*\.jpeg".to_owned(),
                },
            ),
            (
                "result2".to_owned(),
                PathInfo {
                    path: PathBuf::from("result2.jpeg"),
                    match_pattern: r".*\.jpeg".to_owned(),
                },
            ),
        ]),
        "https://github.com/user/style-transfer/tree/1.0.0".to_owned(),
        0.25,        // 250 millicores as frac cores
        1_u64 << 30, // 1GiB in bytes
        Some(Annotation {
            name: "style-transfer".to_owned(),
            description: "This is an example pod.".to_owned(),
            version: "1.0.0".to_owned(),
        }),
        None,
    )
}

pub fn pod_adder(duration_seconds: u8, last_command: &str) -> Result<Pod> {
    Pod::new(
        "alpine:3.14".into(),
        vec![
            "sh".into(),
            "-c".into(),
            formatdoc! {"
                echo 'Adding two numbers'
                echo $(($(cat /tmp/input/left.txt) + $(cat /tmp/input/right.txt))) > /tmp/output/answer.txt
                sleep {duration_seconds}
                {last_command}
            "}.trim().replace('\n', " && "),
        ],
        HashMap::from([
            (
                "left".to_owned(),
                PathInfo {
                    path: PathBuf::from("/tmp/input/left.txt"),
                    match_pattern: r".*\.txt".to_owned(),
                },
            ),
            (
                "right".to_owned(),
                PathInfo {
                    path: PathBuf::from("/tmp/input/right.txt"),
                    match_pattern: r".*\.txt".to_owned(),
                },
            ),
        ]),
        PathBuf::from("/tmp/output"),
        HashMap::from([
            (
                "answer".to_owned(),
                PathInfo {
                    path: PathBuf::from("answer.txt"),
                    match_pattern: r".*\.txt".to_owned(),
                },
            ),
        ]),
        "https://github.com/place/holder".to_owned(),
        0.1,          // 100 millicores as frac cores
        10_u64 << 20, // 10 MiB in bytes
        None,
        None,
    )
}

pub fn pod_job_style(namespace_lookup: &HashMap<String, PathBuf, RandomState>) -> Result<PodJob> {
    PodJob::new(
        pod_style()?.into(),
        Packet(HashMap::from([
            (
                "extra-style".to_owned(),
                PathSet::Unary {
                    blob: Blob {
                        kind: BlobKind::File,
                        location: URI {
                            namespace: "default".to_owned(),
                            path: PathBuf::from("styles/mosaic.t7"),
                        },
                        checksum: String::new(),
                    },
                },
            ),
            (
                "base-input".to_owned(),
                PathSet::Collection {
                    blobs: vec![
                        Blob {
                            kind: BlobKind::File,
                            location: URI {
                                namespace: "default".to_owned(),
                                path: PathBuf::from("styles/style1.t7"),
                            },
                            checksum: String::new(),
                        },
                        Blob {
                            kind: BlobKind::File,
                            location: URI {
                                namespace: "default".to_owned(),
                                path: PathBuf::from("images/subject.jpeg"),
                            },
                            checksum: String::new(),
                        },
                    ],
                },
            ),
        ]))
        .into(),
        URI {
            namespace: "default".to_owned(),
            path: PathBuf::from("output"),
        },
        0.5,         // 500 millicores as frac cores
        2_u64 << 30, // 2GiB in bytes, KiB=<<10, MiB=<<20, GiB=<<30
        namespace_lookup,
        Some(Annotation {
            name: "style-transfer".to_owned(),
            description: "This is an example pod job.".to_owned(),
            version: "0.1.0".to_owned(),
        }),
        Some(HashMap::from([
            ("ZZZ".to_owned(), "PLEASE".to_owned()),
            ("AAA".to_owned(), "SORT".to_owned()),
        ])),
    )
}

pub fn pod_result_style(
    namespace_lookup: &HashMap<String, PathBuf, RandomState>,
) -> Result<PodResult> {
    PodResult::new(
        Some(Annotation {
            name: "style-transfer".to_owned(),
            description: "This is an example pod result.".to_owned(),
            version: "0.0.0".to_owned(),
        }),
        pod_job_style(namespace_lookup)?.into(),
        "simple-endeavour".to_owned(),
        PodStatus::Completed,
        1_737_922_307,
        1_737_925_907,
        namespace_lookup,
    )
}

pub fn pod_job_custom(
    image_reference: &str,
    command: &[String],
    output_spec: HashMap<String, PathInfo, RandomState>,
    namespace_lookup: &HashMap<String, PathBuf, RandomState>,
) -> Result<PodJob> {
    PodJob::new(
        Pod::new(
            image_reference.into(),
            command.to_owned(),
            HashMap::new(),
            PathBuf::from("/tmp/output"),
            output_spec,
            "https://github.com/place/holder".to_owned(),
            0.1,          // 100 millicores as frac cores
            10_u64 << 20, // 10 MiB in bytes
            None,
            None,
        )?
        .into(),
        Packet(HashMap::new()).into(),
        URI {
            namespace: "default".to_owned(),
            path: PathBuf::from("."),
        },
        1.0,          // 1000 millicores as frac cores
        10_u64 << 20, // 2GiB in bytes, KiB=<<10, MiB=<<20, GiB=<<30
        namespace_lookup,
        None,
        None,
    )
}

pub fn pod_jobs_stresser(
    image_reference: &str,
    run_duration_secs: u16,
    success_count: usize,
    error_count: usize,
) -> Result<Vec<Arc<PodJob>>> {
    (1..=(success_count + error_count))
        .map(|i| {
            if i <= success_count {
                return Ok(pod_job_custom(
                    image_reference,
                    &str_to_vec(&format!("stress-ng --cpu 1 --cpu-load 100 --timeout {run_duration_secs} --metrics-brief")),
                    HashMap::new(),
                    &NAMESPACE_LOOKUP_READ_ONLY,
                )?
                .into());
            }
            Ok(pod_job_custom(image_reference, &str_to_vec("sleep crash"), HashMap::new(), &NAMESPACE_LOOKUP_READ_ONLY)?.into())
        })
        .collect::<Result<Vec<_>>>()
}

pub fn pipeline_job_adder(
    pod_duration_seconds: u8,
    fail_pods: &[&str],
    data_dir_filepath: &Path,
    namespace_lookup: &HashMap<String, PathBuf, RandomState>,
) -> Result<PipelineJob> {
    fs::create_dir_all(data_dir_filepath.join("input"))?;
    for i in 1..16 {
        fs::write(
            data_dir_filepath.join(format!("input/{i}.txt")),
            i.to_string(),
        )?;
    }

    let pod_adder_success = Arc::new(pod_adder(pod_duration_seconds, "exit 0")?);
    let pod_adder_failure = Arc::new(pod_adder(pod_duration_seconds, "exit 1")?);
    PipelineJob::new(
        Pipeline::new(
            indoc! {"
                digraph {
                    add_a -> map_left_a
                    add_b -> map_right_a
                    add_c -> map_left_b
                    add_d -> map_right_b
                    { map_left_a map_right_a } -> cartesian_a -> add_e -> map_left_c
                    { map_left_b map_right_b } -> cartesian_b -> add_f -> map_right_c
                    { map_left_c map_right_c } -> cartesian_c -> add_g
                }
            "},
            ["a", "b", "c"]
                .into_iter()
                .flat_map(|k| ["left", "right"].into_iter().map(move |side| (k, side)))
                .map(|(k, side)| {
                    (
                        format!("map_{side}_{k}"),
                        Kernel::MapOperator {
                            map: HashMap::from([("answer".into(), (*side).into())]),
                        },
                    )
                })
                .chain(
                    ["a", "b", "c"]
                        .iter()
                        .map(|k| (format!("cartesian_{k}"), Kernel::JoinOperator)),
                )
                .chain(["a", "b", "c", "d", "e", "f", "g"].iter().map(|k| {
                    let pod_name = format!("add_{k}");
                    let pod_ref = if fail_pods.contains(&pod_name.as_str()) {
                        Arc::clone(&pod_adder_failure)
                    } else {
                        Arc::clone(&pod_adder_success)
                    };
                    (pod_name, Kernel::Pod { r#ref: pod_ref })
                }))
                .collect(),
            &["a", "b", "c", "d"]
                .into_iter()
                .flat_map(|k| ["left", "right"].into_iter().map(move |side| (k, side)))
                .map(|(k, side)| {
                    (
                        format!("{side}_add_{k}"),
                        vec![InputSpecURI {
                            node: format!("add_{k}"),
                            key: (*side).into(),
                        }],
                    )
                })
                .collect(),
            &HashMap::from([(
                "answer".into(),
                OutputSpecURI {
                    node: "add_g".into(),
                    key: "answer".into(),
                },
            )]),
        )?
        .into(),
        &[
            (
                "left_add_a".into(),
                (1..4)
                    .map(|i| PathSet::Unary {
                        blob: Blob {
                            kind: BlobKind::File,
                            location: URI {
                                namespace: "default".into(),
                                path: data_dir_filepath.join(format!("input/{i}.txt")),
                            },
                            checksum: String::new(),
                        },
                    })
                    .collect(),
            ),
            (
                "right_add_a".into(),
                (4..6)
                    .map(|i| PathSet::Unary {
                        blob: Blob {
                            kind: BlobKind::File,
                            location: URI {
                                namespace: "default".into(),
                                path: data_dir_filepath.join(format!("input/{i}.txt")),
                            },
                            checksum: String::new(),
                        },
                    })
                    .collect(),
            ),
        ]
        .into_iter()
        .chain(
            [
                "left_add_b",
                "right_add_b",
                "left_add_c",
                "right_add_c",
                "left_add_d",
                "right_add_d",
            ]
            .iter()
            .enumerate()
            .map(|(i, k)| {
                (
                    (*k).into(),
                    vec![PathSet::Unary {
                        blob: Blob {
                            kind: BlobKind::File,
                            location: URI {
                                namespace: "default".into(),
                                path: data_dir_filepath.join(format!("input/{}.txt", i + 6)),
                            },
                            checksum: String::new(),
                        },
                    }],
                )
            }),
        )
        .collect(),
        &URI {
            namespace: "default".into(),
            path: PathBuf::from(data_dir_filepath.file_name().expect("Relative directory."))
                .join("output/pipeline_adder"),
        },
        namespace_lookup,
    )
}

pub fn container_image_style(binary_location: impl AsRef<Path>) -> Result<TestContainerImage> {
    let build_context_location = PathBuf::from("./tests/extra/example_pod/style_transfer");

    if let Some(parent) = binary_location.as_ref().parent() {
        fs::create_dir_all(parent)?;
    }
    let image_name = format!(
        "{}:0.0.0",
        Generator::with_naming(Name::Plain)
            .next()
            .expect("Name generation overflowed.")
    );
    Command::new("docker")
        .arg("build")
        .arg(&build_context_location)
        .arg("-t")
        .arg(&image_name)
        .stderr(Stdio::inherit())
        .output()?;
    let docker_save = Command::new("docker")
        .arg("save")
        .arg(&image_name)
        .stderr(Stdio::inherit())
        .stdout(Stdio::piped())
        .spawn()?;
    Command::new("gzip")
        .stdin(Stdio::from(
            docker_save.stdout.expect("No pipe data received."),
        ))
        .stderr(Stdio::inherit())
        .stdout(File::create(&binary_location).expect("Failed to open image tarball location."))
        .output()?;
    Command::new("docker")
        .arg("rmi")
        .arg(&image_name)
        .stderr(Stdio::inherit())
        .output()?;

    Ok(TestContainerImage {
        image_name,
        build_context_location,
        binary_location: binary_location.as_ref().to_path_buf(),
    })
}

pub fn pull_image(reference: &str) -> Result<()> {
    Command::new("docker")
        .arg("pull")
        .arg(reference)
        .stderr(Stdio::inherit())
        .stdout(Stdio::inherit())
        .output()?;
    Ok(())
}

// --- util ---

pub fn str_to_vec(v: &str) -> Vec<String> {
    v.split_whitespace().map(String::from).collect()
}

pub struct TestDirs(pub HashMap<String, TempDir>);

impl TestDirs {
    pub fn new(config: &HashMap<String, Option<impl AsRef<Path>>>) -> Result<Self> {
        Ok(Self(
            config
                .iter()
                .map(|(namespace, copy_from)| {
                    let temp_dir = TempDir::with_prefix_in("", "tests/.tmp")?;
                    if let Some(source) = copy_from {
                        Command::new("rsync")
                            .arg("-a")
                            .arg("--delete")
                            .arg(source.as_ref())
                            .arg(temp_dir.path())
                            .output()?;
                        remove_dir_all(temp_dir.path().join("output"))?;
                    }
                    Ok((namespace.clone(), temp_dir))
                })
                .collect::<Result<_>>()?,
        ))
    }
    pub fn namespace_lookup(&self) -> HashMap<String, PathBuf> {
        self.0
            .iter()
            .map(|(key, directory)| (key.clone(), directory.path().into()))
            .collect()
    }
}

pub struct TestContainerImage {
    pub image_name: String,
    pub build_context_location: PathBuf,
    pub binary_location: PathBuf,
}

impl Drop for TestContainerImage {
    fn drop(&mut self) {
        Command::new("docker")
            .arg("rmi")
            .arg(&self.image_name)
            .stderr(Stdio::inherit())
            .output()
            .expect("Failed to teardown container image.");
        fs::remove_file(&self.binary_location).expect("Failed to remove image binary.");
    }
}

pub trait TestSetup: Sized {
    fn save(&self, store: &impl Store) -> Result<()>;
    fn delete(&self, store: &impl Store) -> Result<()>;
    fn load(&self, store: &impl Store) -> Result<Self>;
    fn get_annotation(&self) -> Option<&Annotation>;
    fn get_hash(&self) -> &str;
    fn list(&self, store: &impl Store) -> Result<Vec<ModelInfo>>;
}

impl TestSetup for Pod {
    fn save(&self, store: &impl Store) -> Result<()> {
        store.save_pod(self)
    }
    fn delete(&self, store: &impl Store) -> Result<()> {
        store.delete_pod(&ModelID::Hash {
            r#ref: self.hash.clone(),
        })
    }
    fn load(&self, store: &impl Store) -> Result<Self> {
        let annotation = self.annotation.as_ref().expect("Annotation missing.");
        store.load_pod(&ModelID::Annotation {
            name: annotation.name.clone(),
            version: annotation.version.clone(),
        })
    }
    fn get_annotation(&self) -> Option<&Annotation> {
        self.annotation.as_ref()
    }
    fn get_hash(&self) -> &str {
        &self.hash
    }
    fn list(&self, store: &impl Store) -> Result<Vec<ModelInfo>> {
        store.list_pod()
    }
}

impl TestSetup for PodJob {
    fn save(&self, store: &impl Store) -> Result<()> {
        store.save_pod_job(self)
    }
    fn delete(&self, store: &impl Store) -> Result<()> {
        store.delete_pod_job(&ModelID::Hash {
            r#ref: self.hash.clone(),
        })
    }
    fn load(&self, store: &impl Store) -> Result<Self> {
        let annotation = self.annotation.as_ref().expect("Annotation missing.");
        store.load_pod_job(&ModelID::Annotation {
            name: annotation.name.clone(),
            version: annotation.version.clone(),
        })
    }
    fn get_annotation(&self) -> Option<&Annotation> {
        self.annotation.as_ref()
    }
    fn get_hash(&self) -> &str {
        &self.hash
    }
    fn list(&self, store: &impl Store) -> Result<Vec<ModelInfo>> {
        store.list_pod_job()
    }
}

impl TestSetup for PodResult {
    fn save(&self, store: &impl Store) -> Result<()> {
        store.save_pod_result(self)
    }
    fn delete(&self, store: &impl Store) -> Result<()> {
        store.delete_pod_result(&ModelID::Hash {
            r#ref: self.hash.clone(),
        })
    }
    fn load(&self, store: &impl Store) -> Result<Self> {
        let annotation = self.annotation.as_ref().expect("Annotation missing.");
        store.load_pod_result(&ModelID::Annotation {
            name: annotation.name.clone(),
            version: annotation.version.clone(),
        })
    }
    fn get_annotation(&self) -> Option<&Annotation> {
        self.annotation.as_ref()
    }
    fn get_hash(&self) -> &str {
        &self.hash
    }
    fn list(&self, store: &impl Store) -> Result<Vec<ModelInfo>> {
        store.list_pod_result()
    }
}
