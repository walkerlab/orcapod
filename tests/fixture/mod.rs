use anyhow::Result;
use orcapod::{
    model::{to_yaml, Annotation, Pod, StreamInfo},
    store::{filestore::LocalFileStore, Store},
};
use std::{
    collections::BTreeMap,
    fs,
    ops::{Deref, DerefMut},
    path::PathBuf,
};
use tempfile::tempdir;

#[derive(PartialEq, Clone)]
pub enum Item {
    Pod(Pod),
}

pub enum ItemType {
    Pod,
}

impl Item {
    #[expect(clippy::unwrap_used)]
    pub fn get_name(&self) -> &str {
        match self {
            Self::Pod(pod) => &pod.annotation.as_ref().unwrap().name,
        }
    }

    pub fn get_hash(&self) -> &str {
        match self {
            Self::Pod(pod) => &pod.hash,
        }
    }

    #[expect(clippy::unwrap_used)]
    pub fn get_version(&self) -> &str {
        match self {
            Self::Pod(pod) => &pod.annotation.as_ref().unwrap().version,
        }
    }

    pub fn to_yaml(&self) -> Result<String> {
        match self {
            Self::Pod(pod) => to_yaml(pod),
        }
    }

    #[expect(clippy::unwrap_used)]
    pub fn set_name(&mut self, name: &str) {
        match self {
            Self::Pod(pod) => name.clone_into(&mut pod.annotation.as_mut().unwrap().name),
        }
    }
}

pub fn get_test_item(item_type: &ItemType) -> Result<Item> {
    match item_type {
        ItemType::Pod => Ok(Item::Pod(get_test_pod()?)),
    }
}

pub fn get_test_pod() -> Result<Pod> {
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

#[expect(clippy::unwrap_used)]
impl TestLocalStore {
    /// # Panics
    /// Will panic if fail to fetch stuff related to annotation
    pub fn make_annotation_path(&self, item: &Item) -> PathBuf {
        match item {
            Item::Pod(pod) => self.store.make_annotation_path::<Pod>(
                &pod.hash,
                &pod.annotation.as_ref().unwrap().name,
                &pod.annotation.as_ref().unwrap().version,
            ),
        }
    }

    pub fn make_path(&self, item_type: &ItemType, hash: &str, file_name: &str) -> PathBuf {
        match item_type {
            ItemType::Pod => self.store.make_path::<Pod>(hash, file_name),
        }
    }

    pub fn save_item(&mut self, item: &Item) -> Result<()> {
        match item {
            Item::Pod(pod) => self.store.save_pod(pod),
        }
    }

    pub fn load_item(&mut self, item_type: &ItemType, name: &str, version: &str) -> Result<Item> {
        match item_type {
            ItemType::Pod => Ok(Item::Pod(self.store.load_pod(name, version)?)),
        }
    }

    pub fn list_item(&mut self, item_type: &ItemType) -> Result<&BTreeMap<String, String>> {
        match item_type {
            ItemType::Pod => self.store.list_pod(),
        }
    }

    pub fn delete_item(&mut self, item_type: &ItemType, name: &str, version: &str) -> Result<()> {
        match item_type {
            ItemType::Pod => self.store.delete_pod(name, version),
        }
    }

    pub fn delete_item_annotation(
        &mut self,
        item_type: &ItemType,
        name: &str,
        version: &str,
    ) -> Result<()> {
        match item_type {
            ItemType::Pod => self.store.delete_annotation::<Pod>(name, version),
        }
    }
}

pub fn store_test(store_directory: Option<&str>) -> Result<TestLocalStore> {
    let tmp_directory = String::from(tempdir()?.path().to_string_lossy());
    let store =
        store_directory.map_or_else(|| LocalFileStore::new(tmp_directory), LocalFileStore::new);
    Ok(TestLocalStore { store })
}
