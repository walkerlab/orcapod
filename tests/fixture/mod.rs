#![expect(clippy::expect_used, reason = "Expect OK in tests.")]
#![expect(
    clippy::unwrap_in_result,
    reason = "Expect OK in tests that return result."
)]
#![expect(
    clippy::missing_errors_doc,
    reason = "Integration tests won't be included in documentation."
)]

use orcapod::{
    error::Result,
    model::{
        Annotation, Blob, BlobInterface, FileOrFolder, FolderOnly, Input, Pod, PodJob, StreamInfo,
    },
    store::{filestore::LocalFileStore, ModelID, ModelInfo, Store},
};
use std::{
    collections::BTreeMap,
    fs,
    ops::Deref,
    path::PathBuf,
    process::{Command, Stdio},
};
use tempfile::tempdir;

// --- fixtures ---

pub fn pod_style() -> Result<Pod> {
    Pod::new(
        Some(Annotation {
            name: "style-transfer".to_owned(),
            description: "This is an example pod.".to_owned(),
            version: "1.0.0".to_owned(),
        }),
        "example.server.com/user/style-transfer:1.0.0".to_owned(),
        "python /run.py".to_owned(),
        BTreeMap::from([
            (
                "style".to_owned(),
                StreamInfo {
                    path: PathBuf::from("/input/style.t7"),
                    match_pattern: r".*\.t7".to_owned(),
                },
            ),
            (
                "image".to_owned(),
                StreamInfo {
                    path: PathBuf::from("/input/image.jpeg"),
                    match_pattern: r".*\.jpeg".to_owned(),
                },
            ),
        ]),
        PathBuf::from("/output"),
        BTreeMap::from([(
            "result".to_owned(),
            StreamInfo {
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

pub fn pod_job_style(blob_interface: &impl BlobInterface, build_image: bool) -> Result<PodJob> {
    let pod = pod_style()?;
    if build_image {
        Command::new("docker")
            .arg("build")
            .arg("./tests/example_pod/style_transfer")
            .arg("-t")
            .arg(&pod.image)
            .stderr(Stdio::inherit())
            .output()?;
    }
    PodJob::new(
        Some(Annotation {
            name: "style-transfer".to_owned(),
            description: "This is an example pod job.".to_owned(),
            version: "0.1.0".to_owned(),
        }),
        pod,
        BTreeMap::from([
            (
                "style".to_owned(),
                Input::Unary(Blob {
                    kind: FileOrFolder::File,
                    location: PathBuf::from("styles/mosaic.t7"),
                    checksum: None,
                }),
            ),
            (
                "image".to_owned(),
                Input::Unary(Blob {
                    kind: FileOrFolder::File,
                    location: PathBuf::from("images/dog.jpeg"),
                    checksum: None,
                }),
            ),
        ]),
        Blob {
            kind: FolderOnly::Folder,
            location: PathBuf::from("output"),
            checksum: Some("please_ignore".to_owned()),
        },
        0.5,         // 500 millicores as frac cores
        2_u64 << 30, // 2GiB in bytes, KiB=<<10, MiB=<<20, GiB=<<30
        None,
        blob_interface,
    )
}

pub fn store_test(store_directory: Option<&str>, with_data: bool) -> Result<TestStore> {
    let tmp_directory = String::from(tempdir()?.path().to_string_lossy());
    let store =
        store_directory.map_or_else(|| LocalFileStore::new(tmp_directory), LocalFileStore::new);
    fs::create_dir_all(store.get_directory())?;
    if with_data {
        Command::new("cp")
            .arg("-r")
            .arg("./tests/data")
            .arg(format!(
                "{}/{}",
                store.get_directory().to_string_lossy(),
                LocalFileStore::DEFAULT_DATA_NAMESPACE
            ))
            .output()?;
    }
    Ok(TestStore { store })
}

// --- helper functions ---

pub fn add_storage<T: TestSetup>(model: T, store: &TestStore) -> Result<TestStoredModel<T>> {
    model.save(store)?;
    let model_with_storage = TestStoredModel { store, model };
    Ok(model_with_storage)
}

// --- util ---

#[derive(Debug)]
pub struct TestStore {
    pub store: LocalFileStore,
}

#[derive(Debug)]
pub struct TestStoredModel<'base, T: TestSetup> {
    pub store: &'base TestStore,
    pub model: T,
}

impl Deref for TestStore {
    type Target = LocalFileStore;
    fn deref(&self) -> &Self::Target {
        &self.store
    }
}

impl Drop for TestStore {
    fn drop(&mut self) {
        fs::remove_dir_all(self.store.get_directory()).expect("Failed to teardown store.");
    }
}

impl<'base, T: TestSetup> Drop for TestStoredModel<'base, T> {
    fn drop(&mut self) {
        self.model
            .delete(self.store)
            .expect("Failed to teardown model.");
    }
}

pub trait TestSetup {
    type Target;
    fn save(&self, store: &LocalFileStore) -> Result<()>;
    fn delete(&self, store: &LocalFileStore) -> Result<()>;
    fn load(&self, store: &LocalFileStore) -> Result<Self::Target>;
    fn get_annotation(&self) -> Option<&Annotation>;
    fn get_hash(&self) -> &str;
    fn list(&self, store: &LocalFileStore) -> Result<Vec<ModelInfo>>;
}

impl TestSetup for Pod {
    type Target = Self;
    fn save(&self, store: &LocalFileStore) -> Result<()> {
        store.save_pod(self)
    }
    fn delete(&self, store: &LocalFileStore) -> Result<()> {
        store.delete_pod(&ModelID::Hash(self.hash.clone()))
    }
    fn load(&self, store: &LocalFileStore) -> Result<Self::Target> {
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
    fn list(&self, store: &LocalFileStore) -> Result<Vec<ModelInfo>> {
        store.list_pod()
    }
}

impl TestSetup for PodJob {
    type Target = Self;
    fn save(&self, store: &LocalFileStore) -> Result<()> {
        store.save_pod_job(self)
    }
    fn delete(&self, store: &LocalFileStore) -> Result<()> {
        store.delete_pod_job(&ModelID::Hash(self.hash.clone()))
    }
    fn load(&self, store: &LocalFileStore) -> Result<Self::Target> {
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
    fn list(&self, store: &LocalFileStore) -> Result<Vec<ModelInfo>> {
        store.list_pod_job()
    }
}
