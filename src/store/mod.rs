use crate::model::Pod;
use anyhow::Result;
use std::collections::BTreeMap;

pub struct ItemInfo {
    pub name: String,
    pub hash: String,
    pub version: String,
}

pub trait Store {
    fn save_pod(&mut self, pod: &Pod) -> Result<()>;
    fn load_pod(&mut self, name: &str, version: &str) -> Result<Pod>;
    fn list_pod(&mut self) -> Result<&BTreeMap<String, String>>;
    fn delete_pod(&mut self, name: &str, version: &str) -> Result<()>;
    fn delete_annotation<T>(&mut self, name: &str, version: &str) -> Result<()>;
}

pub mod filestore;
