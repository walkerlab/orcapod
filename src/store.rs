use crate::model::Pod;
use serde_yaml::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub trait OrcaStore {
    fn save_pod(&self, pod: &Pod); // add output later -> Result<(), String>
}

#[derive(Debug)]
pub struct LocalFileStore {
    pub location: PathBuf,
}

impl OrcaStore for LocalFileStore {
    fn save_pod(&self, pod: &Pod) {
        let spec_dir = format!(
            "{}/{}/{}",
            self.location.to_str().unwrap(), // use ? when returning result
            "pod",
            pod.sha256
        );

        let spec_file = PathBuf::from(format!("{}/{}", spec_dir, "spec.yaml"));
        let sig_file = PathBuf::from(format!("{}/{}", spec_dir, "signature.yaml"));
        let annotation_file = format!(
            "{}/{}/{}/{}/{}-{}.yaml",
            self.location.to_str().unwrap(), // use ? when returning result
            "annotation",
            "pod",
            pod.annotation.name,
            pod.sha256,
            pod.annotation.version,
        );

        let mut spec_yaml: String;
        let annotation_yaml: String;

        let field_iter: BTreeMap<String, Value> =
            serde_yaml::from_str(&serde_yaml::to_string(pod).unwrap()).unwrap();

        for (k, v) in &field_iter {
            println!("key: {}, value: {:?}", k, v);
        }
    }
}
