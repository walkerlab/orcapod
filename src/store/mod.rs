use crate::{error::OrcaResult, model::Pod};
use std::collections::BTreeMap;

/// Standard behavior of any store backend supported.
pub trait Store {
    /// How a pod is stored.
    fn save_pod(&self, pod: &Pod) -> OrcaResult<()>;
    /// How to load a stored pod into a model instance.
    fn load_pod(&self, name: &str, version: &str) -> OrcaResult<Pod>;
    /// How to query stored pods.
    fn list_pod(&self) -> OrcaResult<BTreeMap<String, Vec<String>>>;
    /// How to delete a stored pod (does not propagate).
    fn delete_pod(&self, name: &str, version: &str) -> OrcaResult<()>;
}
/// Store implementation on a local filesystem.
pub mod filestore;
