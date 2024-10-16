use crate::model::{Pod, PodJob};
use std::error::Error;

pub struct ItemInfo {
    pub name: String,
    pub hash: String,
    pub version: String,
}

pub trait Store {
    fn save_pod(&self, pod: &Pod) -> Result<(), Box<dyn Error>>;
    fn load_pod(&self, name: &str, version: &str) -> Result<Pod, Box<dyn Error>>;
    fn list_pod(&self) -> Result<Vec<ItemInfo>, Box<dyn Error>>;
    fn delete_pod(&self, name: &str, version: &str) -> Result<(), Box<dyn Error>>;

    fn save_pod_job(&self, pod_job: &PodJob) -> Result<(), Box<dyn Error>>;
    fn load_pod_job(&self, name: &str, version: &str) -> Result<PodJob, Box<dyn Error>>;
    fn list_pod_job(&self) -> Result<Vec<ItemInfo>, Box<dyn Error>>;
    fn delete_pod_job(&self, name: &str, version: &str) -> Result<(), Box<dyn Error>>;
}

pub mod filestore;
