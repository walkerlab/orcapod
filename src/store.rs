use crate::model::Pod;
use serde_yaml::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub trait OrcaStore {
    fn save_pod(&self, pod: &Pod); // add output later -> Result<(), String>
}

pub struct LocalFileStore {
    pub location: PathBuf,
}

impl OrcaStore for LocalFileStore {
    fn save_pod(&self, pod: &Pod) {
        let field_iter: BTreeMap<String, Value> =
            serde_yaml::from_str(&serde_yaml::to_string(pod).unwrap()).unwrap();

        for (k, v) in &field_iter {
            println!("key: {}, value: {:?}", k, v);
        }
    }
}
