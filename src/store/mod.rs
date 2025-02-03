use std::path::Path;

use crate::{
    error::Result,
    model::{Pod, PodJob, StorePointer},
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
    pub name: Option<String>,
    /// A model's version.
    pub version: Option<String>,
    /// A model's hash.
    pub hash: String,
}

/// Standard behavior of any store backend supported.
pub trait ModelStore: DataStore {
    /// Namespace where models will be stored.
    const MODEL_NAMESPACE: &str = "orcapod_model";

    /// Default namespace where user data (inputs/outputs) will be stored.
    /// Mainly use for the data store traits
    const DEFAULT_DATA_NAMESPACE: &str = "orcapod_data";

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

    /// How a pod job is stored.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue storing `pod_job`.
    fn save_pod_job(&self, pod_job: &mut PodJob) -> Result<()>;

    /// How to load a stored pod job into a model instance.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue loading a pod job from the store using `name` and
    /// `version`.
    fn load_pod_job(&self, model_id: &ModelID) -> Result<PodJob>;

    /// How to query stored pod jobs.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue querying metadata from existing pod jobs in the
    /// store.
    fn list_pod_job(&self) -> Result<Vec<ModelInfo>>;

    /// How to explicitly delete a stored pod job and all associated annotations (does not
    /// propagate).
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue deleting a pod job from the store using `name` and
    /// `version`.
    fn delete_pod_job(&self, model_id: &ModelID) -> Result<()>;

    /// How to explicitly delete an annotation.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue deleting an annotation from the store using `name`
    /// and `version`.
    fn delete_annotation<T>(&self, name: &str, version: &str) -> Result<()>;

    ///
    /// # Errors
    /// Will with orca error if fail to save
    fn save_store_pointer(&self, store_pointer: &StorePointer) -> Result<()>;

    /// Load the latest store pointer
    ///
    /// # Errors
    /// Will return orca error if fail to load latest store pointer
    fn load_store_pointer(&self, store_name: &str) -> Result<StorePointer>;

    /// List all avaliable store pointers if there are any
    ///
    /// # Errors
    /// Will return `Err` if there is an issue querying metadata from existing store pointers in the store.
    fn list_store_pointer(&self) -> Result<Vec<ModelInfo>>;

    /// Delete store pointer by ``model_id``
    ///
    /// # Errors
    /// Will return error if failed to delete the store pointer
    fn delete_store_pointer(&self, model_id: &ModelID) -> Result<()>;
}

/// Same as blob interface, but renamed due to possible additional of features for store pointer.
pub trait DataStore: Sized {
    /// How to evaluate a checksum of a BLOB.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue computing the checksum of a BLOB.
    fn compute_checksum(&self, path: &dyn AsRef<Path>) -> Result<String>;

    ///
    /// # Errors
    /// Will return invalid uri if file store cannot be rebuilt given the uri
    fn from_uri(uri: &str) -> Result<Self>;

    /// Get the uri string to reconstruct the store later
    ///
    fn get_uri(&self) -> String;

    /// Function to read file into memory
    ///
    /// # Errors
    ///
    /// Will error out with standard ``io::errors``
    fn load_file(&self, path: impl AsRef<Path>) -> Result<Vec<u8>>;
}

/// Store implementation on a local filesystem.
pub mod filestore;
