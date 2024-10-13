use crate::model::Pod;
use std::{collections::BTreeMap, error::Error};

pub trait Store {
    fn save_pod(&self, pod: &Pod) -> Result<(), Box<dyn Error>>;
    fn load_pod(&self, name: &str, version: &str) -> Result<Pod, Box<dyn Error>>;
    fn list_pod(&self) -> Result<BTreeMap<String, Vec<String>>, Box<dyn Error>>;
    fn delete_pod(&self, name: &str, version: &str) -> Result<(), Box<dyn Error>>;
}

pub mod filestore;
