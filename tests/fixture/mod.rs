use anyhow::Result;
use orcapod::{
    model::{Annotation, Pod, StreamInfo},
    store::{filestore::LocalFileStore, Store},
};
use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    ops::{Deref, DerefMut},
    path::PathBuf,
};
use tempfile::tempdir;

pub fn pod_style() -> Result<Pod> {
    Pod::new(
        Annotation {
            name: "style-transfer".to_owned(),
            description: "This is an example pod.".to_owned(),
            version: "0.67.0".to_owned(),
        },
        "https://github.com/zenml-io/zenml/tree/0.67.0".to_owned(),
        "zenmldocker/zenml-server:0.67.0".to_owned(),
        "tail -f /dev/null".to_owned(),
        BTreeMap::from([
            (
                "painting".to_owned(),
                StreamInfo {
                    path: PathBuf::from("/input/painting.png"),
                    match_pattern: "/input/painting.png".to_owned(),
                },
            ),
            (
                "image".to_owned(),
                StreamInfo {
                    path: PathBuf::from("/input/image.png"),
                    match_pattern: "/input/image.png".to_owned(),
                },
            ),
        ]),
        PathBuf::from("/output"),
        BTreeMap::from([(
            "styled".to_owned(),
            StreamInfo {
                path: PathBuf::from("./styled.png"),
                match_pattern: "./styled.png".to_owned(),
            },
        )]),
        0.25,                // 250 millicores as frac cores
        (2_u64) * (1 << 30), // 2GiB in bytes
        None,
    )
}

#[derive(Debug)]
pub struct TestLocalStore {
    store: LocalFileStore,
}

impl Deref for TestLocalStore {
    type Target = LocalFileStore;
    fn deref(&self) -> &Self::Target {
        &self.store
    }
}

impl DerefMut for TestLocalStore {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.store
    }
}

#[expect(clippy::expect_used)]
impl Drop for TestLocalStore {
    fn drop(&mut self) {
        fs::remove_dir_all(self.store.directory.as_path()).expect("Failed to teardown store.");
    }
}

pub fn store_test(store_directory: Option<&str>) -> Result<TestLocalStore> {
    let tmp_directory = String::from(tempdir()?.path().to_string_lossy());
    let store =
        store_directory.map_or_else(|| LocalFileStore::new(tmp_directory), LocalFileStore::new);
    Ok(TestLocalStore { store })
}

#[derive(Debug)]
pub struct TestLocallyStoredPod<'base> {
    pub store: &'base mut TestLocalStore,
    pod: Pod,
}

pub fn add_pod_storage(pod: Pod, store: &mut TestLocalStore) -> Result<TestLocallyStoredPod> {
    impl<'base> Deref for TestLocallyStoredPod<'base> {
        type Target = Pod;
        fn deref(&self) -> &Self::Target {
            &self.pod
        }
    }

    #[expect(clippy::unwrap_used)]
    impl<'base> Drop for TestLocallyStoredPod<'base> {
        fn drop(&mut self) {
            self.store
                .delete_pod(
                    &self.pod.annotation.as_ref().unwrap().name,
                    &self.pod.annotation.as_ref().unwrap().version,
                )
                .unwrap();
        }
    }

    let pod_with_storage = TestLocallyStoredPod { store, pod };
    pod_with_storage.store.save_pod(&pod_with_storage.pod)?;
    Ok(pod_with_storage)
}
