use crate::model::{Annotation, Pod};

pub trait Store {
    fn store_annotation(&self, annotation: &Annotation, owner_hash: &str) -> Result<(), String>;

    fn read_annotation(&self, hash: &str) -> Annotation;

    fn store_pod(&self, pod: &Pod) -> Result<(), String>;

    fn read_pod(&self, hash: &str) -> Pod;
}
