#![expect(
    clippy::expect_used,
    clippy::missing_errors_doc,
    missing_docs,
    clippy::missing_panics_doc,
    clippy::unwrap_in_result,
    reason = "OK in tests."
)]

use names::{Generator, Name};
use orcapod::uniffi::{
    error::Result,
    model::{
        Annotation,
        packet::{Blob, BlobKind, Packet, PathInfo, PathSet, URI},
        pod::{Pod, PodJob, PodResult},
    },
    orchestrator::PodStatus,
    store::{ModelID, ModelInfo, Store},
};
use std::{
    collections::HashMap,
    fs::{self, File},
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
        "python /run.py".to_owned(),
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
        HashMap::from([(
            "result".to_owned(),
            PathInfo {
                path: PathBuf::from("./result.jpeg"),
                match_pattern: r".*\.jpeg".to_owned(),
            },
        )]),
        "https://github.com/user/style-transfer/tree/1.0.0".to_owned(),
        0.25,        // 250 millicores as frac cores
        1_u64 << 30, // 1GiB in bytes
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
    )
}

pub fn pod_custom(
    image_reference: &str,
    command: &str,
    input_spec: HashMap<String, PathInfo, RandomState>,
) -> Result<Pod> {
    Pod::new(
        None,
        image_reference.into(),
        command.into(),
        input_spec,
        PathBuf::from("/tmp/output"),
        HashMap::new(),
        "https://github.com/place/holder".to_owned(),
        0.1,          // 100 millicores as frac cores
        50_u64 << 20, // 10 MiB in bytes
        None,
    )
}

pub fn pod_job_custom(
    pod: &Pod,
    input_packet: Packet,
    namespace_lookup: &HashMap<String, PathBuf, RandomState>,
) -> Result<PodJob> {
    PodJob::new(
        None,
        Arc::new(pod.clone()),
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
                    &pod_custom(
                        image_reference,
                        &format!("stress-ng --cpu 1 --cpu-load 100 --timeout {run_duration_secs} --metrics-brief"),
                        HashMap::new()
                    )?,
                    HashMap::new(),
                    &NAMESPACE_LOOKUP_READ_ONLY,
                )?
                .into());
            }
            Ok(pod_job_custom(
                &pod_custom(image_reference, "sleep crash", HashMap::new())?,
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

// --- util ---

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
