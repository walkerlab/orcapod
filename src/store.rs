use crate::model::Pod;
use std::path::PathBuf;

pub trait OrcaStore {
    // fn save_pod(p: Pod) -> Result<(), String>;
    fn save_pod(&self, pod: Pod);
}

pub struct LocalFileStore {
    pub location: PathBuf,
}

impl OrcaStore for LocalFileStore {
    fn save_pod(&self, pod: Pod) {
        println!("iterate over fields here");
    }
}
