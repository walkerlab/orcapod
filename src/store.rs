use crate::model::{Annotation, Pod};

pub trait Store {
    fn store_annotation(&self, annotation: &Annotation, owner_hash: &str) -> Result<(), String>;

    fn load_annotation(&self, hash: &str) -> Result<Annotation, String>;

    fn store_pod(&self, pod: &Pod) -> Result<(), String>;

    fn load_pod(&self, hash: &str) -> Result<Pod, String>;
}
