use std::path::Path;

use crate::{
    error::Result,
    model::{Pod, PodJob},
};

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
pub trait ModelStore: Sized + DataStore {
    /// How to delete only annotation, which will leave the item untouched
    /// How to explicitly delete an annotation.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue deleting an annotation from the store using `name`
    /// and `version`.
    fn delete_annotation<T>(&self, name: &str, version: &str) -> Result<()>;

    /// How a pod is stored
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

    /// Save ``pod_job`` to storage
    ///
    /// # Errors
    /// Return error if failed to save pod for some reason, either encoding or ioerror
    fn save_pod_job(&self, pod_job: &PodJob) -> Result<()>;

    /// Load ``pod_job`` from storage given an ``model_id``
    ///
    /// # Errors
    /// Will return error if fail to load pod
    fn load_pod_job(&self, model_id: &ModelID) -> Result<PodJob>;

    /// List all ``pod_job``
    ///
    /// # Errors
    /// Will return error if fail to get all pods annotations
    fn list_pod_job(&self) -> Result<Vec<ModelInfo>>;

    /// Delete job by ``model_id``
    ///
    /// # Errors
    /// Will return error if failed to delete the pod job
    fn delete_pod_job(&self, model_id: &ModelID) -> Result<()>;

    /// Will delete everything store
    ///
    /// # Errors
    /// Will return orca error if failed to tear down store
    fn wipe(&self) -> Result<()>;
}

/// Trait to be implemented by file stores
pub trait DataStore: Sized {
    ///
    /// # Errors
    /// Will return invalid uri if file store cannot be rebuilt given the uri
    fn from_uri(uri: &str) -> Result<Self>;

    /// Get the uri string to reconstruct the store later
    ///
    fn get_uri(&self) -> String;

    /// Computes the checksum for a given file or folder using merkle tree
    ///
    /// # Errors
    /// Return ``merkle_hash`` errors
    fn compute_checksum_for_file_or_dir(&self, path: impl AsRef<Path>) -> Result<String>;

    /// Function to read file into memory
    ///
    /// # Errors
    ///
    /// Will error out with standard ``io::errors``
    fn load_file(&self, path: impl AsRef<Path>) -> Result<Vec<u8>>;

    /// Save file to local file store, will error out if file already exist
    ///
    /// # Errors
    /// Will error out with standard ``io::errors``
    fn save_file(&self, path: impl AsRef<Path>, content: Vec<u8>) -> Result<()>;
}

/// Store implementation on a local filesystem.
pub mod localstore;
