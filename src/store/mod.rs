use crate::model::Pod;
use anyhow::Result;
use std::collections::BTreeMap;

pub enum ItemKey {
    NameVer(String, String),
    Hash(String),
}

pub trait Store {
    fn save_pod(&mut self, pod: &Pod) -> Result<()>;
    fn load_pod(&mut self, item_key: &ItemKey) -> Result<Pod>;
    fn list_pod(&mut self) -> Result<&BTreeMap<String, String>>;
    fn delete_pod(&mut self, item_key: &ItemKey) -> Result<()>;
    fn delete_annotation<T>(&mut self, name: &str, version: &str) -> Result<()>;
}

pub mod filestore;
