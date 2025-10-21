#![expect(
    clippy::expect_used,
    clippy::missing_errors_doc,
    missing_docs,
    clippy::missing_panics_doc,
    clippy::unwrap_in_result,
    reason = "OK in tests."
)]

use names::{Generator, Name};
use orcapod::{
    error::Result,
    model::{
        Annotation,
        packet::{Blob, BlobKind, Packet, PathInfo, PathSet, URI},
        pipeline::{Kernel, NodeURI, Pipeline, PipelineJob},
        pod::{Pod, PodJob, PodResult, PodStatus, RecommendedSpecs},
    },
    operator::MapOperator,
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
        Some(Annotation {
            name: "style-transfer".to_owned(),
            description: "This is an example pod.".to_owned(),
            version: "1.0.0".to_owned(),
        }),
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
        RecommendedSpecs {
            cpus: 0.25,
            memory: 1_u64 << 30,
        },
        None,
    )
}

pub fn pod_job_style(namespace_lookup: &HashMap<String, PathBuf, RandomState>) -> Result<PodJob> {
    PodJob::new(
        Some(Annotation {
            name: "style-transfer".to_owned(),
            description: "This is an example pod job.".to_owned(),
            version: "0.1.0".to_owned(),
        }),
        pod_style()?.into(),
        HashMap::from([
            (
                "extra-style".to_owned(),
                PathSet::Unary(Blob {
                    kind: BlobKind::File,
                    location: URI {
                        namespace: "default".to_owned(),
                        path: PathBuf::from("styles/mosaic.t7"),
                    },
                    checksum: String::new(),
                }),
            ),
            (
                "base-input".to_owned(),
                PathSet::Collection(vec![
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
                ]),
            ),
        ]),
        URI {
            namespace: "default".to_owned(),
            path: PathBuf::from("output"),
        },
        0.5,         // 500 millicores as frac cores
        2_u64 << 30, // 2GiB in bytes, KiB=<<10, MiB=<<20, GiB=<<30
        Some(HashMap::from([
            ("ZZZ".to_owned(), "PLEASE".to_owned()),
            ("AAA".to_owned(), "SORT".to_owned()),
        ])),
        namespace_lookup,
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
        "Example logs".to_owned(),
    )
}

pub fn pod_custom(
    image_reference: &str,
    command: Vec<String>,
    input_spec: HashMap<String, PathInfo, RandomState>,
) -> Result<Pod> {
    Pod::new(
        None,
        image_reference.into(),
        command,
        input_spec,
        PathBuf::from("/tmp/output"),
        HashMap::new(),
        RecommendedSpecs {
            cpus: 0.1,
            memory: 50_u64 << 20,
        },
        None,
    )
}

pub fn pod_job_custom(
    pod: Pod,
    input_packet: Packet,
    namespace_lookup: &HashMap<String, PathBuf, RandomState>,
) -> Result<PodJob> {
    PodJob::new(
        None,
        pod.into(),
        input_packet,
        URI {
            namespace: "default".to_owned(),
            path: PathBuf::from("."),
        },
        1.0,          // 1000 millicores as frac cores
        50_u64 << 20, // 2GiB in bytes, KiB=<<10, MiB=<<20, GiB=<<30
        None,
        namespace_lookup,
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
                    pod_custom(
                        image_reference,
                        str_to_vec(&format!("stress-ng --cpu 1 --cpu-load 100 --timeout {run_duration_secs} --metrics-brief")),
                        HashMap::new()
                    )?,
                    HashMap::new(),
                    &NAMESPACE_LOOKUP_READ_ONLY,
                )?
                .into());
            }
            Ok(pod_job_custom(
                pod_custom(image_reference, str_to_vec("sleep crash"), HashMap::new())?,
                HashMap::new(),
                &NAMESPACE_LOOKUP_READ_ONLY,
            )?
            .into())
        })
        .collect::<Result<Vec<_>>>()
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

// Pipeline Fixture
pub fn combine_txt_pod(pod_name: &str) -> Result<Pod> {
    Pod::new(
        Some(Annotation {
            name: pod_name.to_owned(),
            description: "Takes two input files, remove the final next line and combine them"
                .to_owned(),
            version: "1.0.0".to_owned(),
        }),
        "alpine:3.14".to_owned(),
        vec![
            "sh".into(),
            "-c".into(),
            format!(
                "printf '%s %s\\n' \"$(cat input/input_1.txt | head -c -1)\" \"$(cat input/input_2.txt | head -c -1)\" > /output/output.txt"
            ),
        ],
        HashMap::from([
            (
                "input_1".to_owned(),
                PathInfo {
                    path: PathBuf::from("/input/input_1.txt"),
                    match_pattern: r".*\.txt".to_owned(),
                },
            ),
            (
                "input_2".into(),
                PathInfo {
                    path: PathBuf::from("/input/input_2.txt"),
                    match_pattern: r".*\.txt".to_owned(),
                },
            ),
        ]),
        PathBuf::from("/output"),
        HashMap::from([(
            "output".to_owned(),
            PathInfo {
                path: PathBuf::from("output.txt"),
                match_pattern: r".*\.txt".to_owned(),
            },
        )]),
        RecommendedSpecs {
            cpus: 0.25,
            memory: 128_u64 << 20,
        },
        None,
    )
}

#[expect(clippy::too_many_lines, reason = "OK in tests.")]
pub fn pipeline() -> Result<Pipeline> {
    // Create a simple pipeline where the functions job is to add append their name into the input file
    // Structure: A -> Mapper -> Joiner -> B -> Mapper -> C, D -> Mapper -> Joiner

    // Create the kernel map
    let mut kernel_map = HashMap::new();

    // Insert the pod into the kernel map
    for pod_name in ["A", "B", "C", "D", "E"] {
        kernel_map.insert(pod_name.into(), combine_txt_pod(pod_name)?.into());
    }

    let output_to_input_1 = Arc::new(MapOperator::new(HashMap::from([(
        "output".to_owned(),
        "input_1".to_owned(),
    )]))?);

    let output_to_input_2 = Arc::new(MapOperator::new(HashMap::from([(
        "output".to_owned(),
        "input_2".to_owned(),
    )]))?);

    // Create a mapper for A, B, and C
    kernel_map.insert(
        "pod_a_mapper".into(),
        Kernel::MapOperator {
            mapper: Arc::clone(&output_to_input_1),
        },
    );
    kernel_map.insert(
        "pod_b_mapper".into(),
        Kernel::MapOperator {
            mapper: Arc::clone(&output_to_input_2),
        },
    );
    kernel_map.insert(
        "pod_c_mapper".into(),
        Kernel::MapOperator {
            mapper: Arc::clone(&output_to_input_1),
        },
    );
    kernel_map.insert(
        "pod_d_mapper".into(),
        Kernel::MapOperator {
            mapper: Arc::clone(&output_to_input_2),
        },
    );

    for joiner_name in ['c', 'd', 'e'] {
        kernel_map.insert(format!("pod_{joiner_name}_joiner"), Kernel::JoinOperator);
    }

    // Write all the edges in DOT format
    let dot = "
        digraph {
        A -> pod_a_mapper -> pod_c_joiner;
        B -> pod_b_mapper -> pod_c_joiner;
        pod_c_joiner -> C -> pod_c_mapper-> pod_e_joiner;
        D -> pod_d_mapper -> pod_e_joiner;
        pod_e_joiner -> E;
        }
    ";

    Pipeline::new(
        dot,
        &kernel_map,
        HashMap::from([
            (
                "where".into(),
                vec![NodeURI {
                    node_id: "A".into(),
                    key: "input_1".into(),
                }],
            ),
            (
                "is".into(),
                vec![NodeURI {
                    node_id: "A".into(),
                    key: "input_2".into(),
                }],
            ),
            (
                "the".into(),
                vec![NodeURI {
                    node_id: "B".into(),
                    key: "input_1".into(),
                }],
            ),
            (
                "cat_color".into(),
                vec![NodeURI {
                    node_id: "B".into(),
                    key: "input_2".into(),
                }],
            ),
            (
                "cat".into(),
                vec![NodeURI {
                    node_id: "D".into(),
                    key: "input_1".into(),
                }],
            ),
            (
                "action".into(),
                vec![NodeURI {
                    node_id: "D".into(),
                    key: "input_2".into(),
                }],
            ),
        ]),
        HashMap::from([(
            "output".to_owned(),
            NodeURI {
                node_id: "E".into(),
                key: "output".into(),
            },
        )]),
        Some(Annotation {
            name: "test".into(),
            version: "0.0.0".into(),
            description: "Test pipeline".into(),
        }),
    )
}

#[expect(clippy::implicit_hasher, reason = "Could be a false positive?")]
pub fn pipeline_job(namespace_lookup: &HashMap<String, PathBuf>) -> Result<PipelineJob> {
    // Create a simple pipeline_job
    let namespace: String = "default".into();
    PipelineJob::new(
        pipeline()?.into(),
        &HashMap::from([
            (
                "where".into(),
                vec![PathSet::Unary(Blob {
                    kind: BlobKind::File,
                    location: URI {
                        namespace: namespace.clone(),
                        path: "input_txt/Where.txt".into(),
                    },
                    checksum: String::new(),
                })],
            ),
            (
                "is".into(),
                vec![PathSet::Unary(Blob {
                    kind: BlobKind::File,
                    location: URI {
                        namespace: namespace.clone(),
                        path: "input_txt/is.txt".into(),
                    },
                    checksum: String::new(),
                })],
            ),
            (
                "the".into(),
                vec![PathSet::Unary(Blob {
                    kind: BlobKind::File,
                    location: URI {
                        namespace: namespace.clone(),
                        path: "input_txt/the.txt".into(),
                    },
                    checksum: String::new(),
                })],
            ),
            (
                "cat_color".into(),
                vec![
                    PathSet::Unary(Blob {
                        kind: BlobKind::File,
                        location: URI {
                            namespace: namespace.clone(),
                            path: "input_txt/black.txt".into(),
                        },
                        checksum: String::new(),
                    }),
                    PathSet::Unary(Blob {
                        kind: BlobKind::File,
                        location: URI {
                            namespace: namespace.clone(),
                            path: "input_txt/tabby.txt".into(),
                        },
                        checksum: String::new(),
                    }),
                ],
            ),
            (
                "cat".into(),
                vec![PathSet::Unary(Blob {
                    kind: BlobKind::File,
                    location: URI {
                        namespace: namespace.clone(),
                        path: "input_txt/cat.txt".into(),
                    },
                    checksum: String::new(),
                })],
            ),
            (
                "action".into(),
                vec![
                    PathSet::Unary(Blob {
                        kind: BlobKind::File,
                        location: URI {
                            namespace: namespace.clone(),
                            path: "input_txt/hiding.txt".into(),
                        },
                        checksum: String::new(),
                    }),
                    PathSet::Unary(Blob {
                        kind: BlobKind::File,
                        location: URI {
                            namespace,
                            path: "input_txt/playing.txt".into(),
                        },
                        checksum: String::new(),
                    }),
                ],
            ),
        ]),
        URI {
            namespace: "default".to_owned(),
            path: PathBuf::from("pipeline_output"),
        },
        namespace_lookup,
    )
}

// --- util ---

pub fn str_to_vec(v: &str) -> Vec<String> {
    v.split_whitespace().map(String::from).collect()
}

pub struct TestDirs(pub HashMap<String, TempDir>);

impl TestDirs {
    pub fn new(config: &HashMap<String, Option<impl AsRef<Path>>>) -> Result<Self> {
        // Check if .tmp exists if not create it
        fs::create_dir_all("tests/.tmp")?;
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
        store.delete_pod(&ModelID::Hash(self.hash.clone()))
    }
    fn load(&self, store: &impl Store) -> Result<Self> {
        let annotation = self.annotation.as_ref().expect("Annotation missing.");
        store.load_pod(&ModelID::Annotation(
            annotation.name.clone(),
            annotation.version.clone(),
        ))
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
        store.delete_pod_job(&ModelID::Hash(self.hash.clone()))
    }
    fn load(&self, store: &impl Store) -> Result<Self> {
        let annotation = self.annotation.as_ref().expect("Annotation missing.");
        store.load_pod_job(&ModelID::Annotation(
            annotation.name.clone(),
            annotation.version.clone(),
        ))
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
        store.delete_pod_result(&ModelID::Hash(self.hash.clone()))
    }
    fn load(&self, store: &impl Store) -> Result<Self> {
        let annotation = self.annotation.as_ref().expect("Annotation missing.");
        store.load_pod_result(&ModelID::Annotation(
            annotation.name.clone(),
            annotation.version.clone(),
        ))
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

impl TestSetup for Pipeline {
    fn save(&self, store: &impl Store) -> Result<()> {
        store.save_pipeline(self)
    }
    fn delete(&self, store: &impl Store) -> Result<()> {
        store.delete_pipeline(&ModelID::Hash(self.hash.clone()))
    }
    fn load(&self, store: &impl Store) -> Result<Self> {
        let annotation = self.annotation.as_ref().expect("Annotation missing.");
        store.load_pipeline(&ModelID::Annotation(
            annotation.name.clone(),
            annotation.version.clone(),
        ))
    }
    fn get_annotation(&self) -> Option<&Annotation> {
        self.annotation.as_ref()
    }
    fn get_hash(&self) -> &str {
        &self.hash
    }
    fn list(&self, store: &impl Store) -> Result<Vec<ModelInfo>> {
        store.list_pipeline()
    }
}
