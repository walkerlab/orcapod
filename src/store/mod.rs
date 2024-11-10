use crate::{error::Result, model::Pod};

/// Options for identifying a model.
pub enum ModelID {
    /// Identifying by the hash value of a model as a string.
    Hash(String),
    /// Identifying by the `(name, version)` of an annotation for a model as strings.
    Annotation(String, String),
}

/// Metadata for a model.
#[derive(Debug, PartialEq, Eq)]
pub struct ModelInfo {
    /// A model's name.
    pub name: String,
    /// A model's version.
    pub version: String,
    /// A model's hash.
    pub hash: String,
}

/// Standard behavior of any store backend supported.
pub trait Store {
    /// How a pod is stored.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue storing `pod`.
    fn save_pod(&self, pod: &Pod) -> Result<()>;
    /// How to load a stored pod into a model instance.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue loading a pod from the store using `name` and
    /// `version`.
    fn load_pod(&self, model_id: &ModelID) -> Result<Pod>;
    /// How to query stored pods.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue querying metadata from existing pods in the store.
    fn list_pod(&self) -> Result<Vec<ModelInfo>>;
    /// How to explicitly delete a stored pod and all associated annotations (does not propagate).
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue deleting a pod from the store using `name` and
    /// `version`.
    fn delete_pod(&self, model_id: &ModelID) -> Result<()>;
    /// How to explicitly delete an annotation.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue deleting an annotation from the store using `name`
    /// and `version`.
    fn delete_annotation<T>(&self, name: &str, version: &str) -> Result<()>;
}
/// Store implementation on a local filesystem.
pub mod filestore;
