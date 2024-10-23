use crate::{error::OrcaResult, model::Pod};
use std::collections::BTreeMap;

pub trait Store {
    fn save_pod(&self, pod: &Pod) -> OrcaResult<()>;
    fn load_pod(&self, name: &str, version: &str) -> OrcaResult<Pod>;
    fn list_pod(&self) -> OrcaResult<BTreeMap<String, Vec<String>>>;
    fn delete_pod(&self, name: &str, version: &str) -> OrcaResult<()>;
}

pub mod filestore;
