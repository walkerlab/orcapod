use crate::{
    crypto::hash_bytes,
    error::{Kind, OrcaError, Result},
    store::{filestore::LocalFileStore, DataStore, ModelStore},
    util::{get_type_name, hash},
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{collections::BTreeMap, path::PathBuf, result};
/// Converts a model instance into a consistent yaml.
///
/// # Errors
///
/// Will return `Err` if there is an issue converting an `instance` into YAML (w/o annotation).
pub fn to_yaml<T: Serialize>(instance: &T) -> Result<String> {
    let mut yaml = serde_yaml::to_string(instance)?;
    yaml.insert_str(0, &format!("class: {}\n", get_type_name::<T>())); // replace class at top

    Ok(yaml)
}

// --- core model structs ---

/// A reusable, containerized computational unit.
#[derive(Serialize, Deserialize, Debug, PartialEq, Default, Clone)]
pub struct Pod {
    /// Metadata that doesn't affect reproducibility.
    #[serde(skip)]
    pub annotation: Option<Annotation>,
    /// Unique id based on reproducibility.
    #[serde(skip)]
    pub hash: String,
    image: String,
    command: String,
    input_stream_map: BTreeMap<String, StreamInfo>,
    output_dir: PathBuf,
    output_stream_map: BTreeMap<String, StreamInfo>,
    source_commit_url: String,
    recommended_cpus: f32,
    recommended_memory: u64,
    required_gpu: Option<GPURequirement>,
}

impl Pod {
    /// Construct a new pod instance.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue initializing a `Pod` instance.
    pub fn new(
        annotation: Option<Annotation>,
        source_commit_url: String,
        image: String,
        command: String,
        input_stream_map: BTreeMap<String, StreamInfo>,
        output_dir: PathBuf,
        output_stream_map: BTreeMap<String, StreamInfo>,
        recommended_cpus: f32,
        recommended_memory: u64,
        required_gpu: Option<GPURequirement>,
    ) -> Result<Self> {
        let pod_no_hash = Self {
            annotation,
            hash: String::new(),
            source_commit_url,
            image,
            command,
            input_stream_map,
            output_dir,
            output_stream_map,
            recommended_cpus,
            recommended_memory,
            required_gpu,
        };
        Ok(Self {
            hash: hash_bytes(to_yaml(&pod_no_hash)?),
            ..pod_no_hash
        })
    }
}

fn serialize_pod<S>(pod: &Pod, serializer: S) -> result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&pod.hash)
}

fn deserialize_pod<'de, D>(deserializer: D) -> result::Result<Pod, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Pod {
        hash: String::deserialize(deserializer)?,
        ..Pod::default()
    })
}

/// A compute job that specifies resource requests and input/output targets.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct PodJob {
    /// Metadata that doesn't affect reproducibility.
    #[serde(skip)]
    pub annotation: Option<Annotation>,
    /// Unique id based on reproducibility.
    #[serde(skip)]
    pub hash: String,
    /// A pod to base the pod job on.
    #[serde(serialize_with = "serialize_pod", deserialize_with = "deserialize_pod")]
    pub pod: Pod,
    /// Map stream ids to an input in user data target.
    pub input_stream_path: BTreeMap<String, Input>,
    /// Map output directory to a folder in user data target.
    pub output_stream_path: Blob<FolderOnly>,
    cpu_limit: f32,
    memory_limit: u64,
}

impl PodJob {
    /// Construct a new pod job instance.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue initializing a `PodJob` instance.
    pub fn new(
        annotation: Option<Annotation>,
        pod: Pod,
        input_stream_path: BTreeMap<String, Input>,
        output_stream_path: Blob<FolderOnly>,
        cpu_limit: f32,
        memory_limit: u64,
    ) -> Result<Self> {
        let pod_job_no_hash = Self {
            annotation,
            hash: String::new(),
            pod,
            input_stream_path,
            output_stream_path,
            cpu_limit,
            memory_limit,
        };
        Ok(Self {
            hash: hash(to_yaml(&pod_job_no_hash)?),
            ..pod_job_no_hash
        })
    }
}

impl PodJob {
    /// Helper function to compute the hash for the input_stream_path that is typically used when saving the model
    /// This is using last min save, basically do not compute the checksum, until it is actually written, but can be used
    /// before hand in memory.
    ///
    /// # Errors
    /// Error if fails to compute checksum, mainly due to FileIO
    ///
    pub fn compute_checksum_for_input_stream_path<T: ModelStore>(
        &mut self,
        model_store: &T,
    ) -> Result<()> {
        for (_, input) in self.input_stream_path.iter_mut() {
            input.compute_checksum(model_store)?
        }
        Ok(())
    }
}

/// Model object that contains a ``BTreeMap``that maps store names to the actual URI use to reconstruct the stores
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct StorePointer {
    /// Version tag to uniquely identify
    #[serde(skip)]
    pub annotation: Annotation,
    #[serde(skip)]
    /// hash identity, for now it is just the uri
    pub hash: String,
    /// Uri path to the store
    pub uri: String,
}

impl StorePointer {
    /// Function to create new store pointer and compute the hash
    ///
    /// # Errors
    /// Return serialization error if something went wrong.
    pub fn new(annotation: Annotation, uri: String) -> Result<Self> {
        let mut store_pointer = Self {
            annotation,
            uri,
            hash: String::new(),
        };

        store_pointer.hash = hash(&to_yaml(&store_pointer)?);
        Ok(store_pointer)
    }

    /// Function to rebuild the store based on self
    ///
    /// # Errors
    /// Will fail if rebuilding of the store access struct fails
    pub fn get_store(&self) -> Result<impl DataStore> {
        // Load the yaml into a Btreemap, pull out the class, then build the store

        let storage_class_name = self.uri.split("::").collect::<Vec<&str>>()[0];

        match storage_class_name {
            "LocalStore" => Ok(LocalFileStore::from_uri(&self.uri)?),
            _ => Err(OrcaError::from(Kind::UnsupportedFileStorage {
                data_storage_type: storage_class_name.to_owned(),
            })),
        }
    }
}

// --- util types ---

/// Standard metadata structure for all model instances.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Eq, Clone)]
pub struct Annotation {
    /// A unique name.
    pub name: String,
    /// A unique semantic version.
    pub version: String,
    /// A long form description.
    pub description: String,
}
/// Specification for GPU requirements in computation.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct GPURequirement {
    /// GPU model specification.
    pub model: GPUModel,
    /// Manufacturer recommended memory.
    pub recommended_memory: u64,
    /// Number of GPU cards required.
    pub count: u16,
}
/// GPU model specification.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum GPUModel {
    /// NVIDIA-manufactured card where `String` is the specific model e.g. ???
    NVIDIA(String),
    /// AMD-manufactured card where `String` is the specific model e.g. ???
    AMD(String),
}
/// Streams are named and represent an abstraction for the file(s) that represent some particular
/// data.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct StreamInfo {
    /// Path to stream file.
    pub path: PathBuf,
    /// Naming pattern for the stream.
    pub match_pattern: String,
}
/// Input options sourced from user data target.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum Input {
    /// A single BLOB.
    Unary(Blob<FileOrFolder>),
    /// A series of BLOBs.
    Collection(Vec<Blob<FileOrFolder>>),
}

impl Input {
    fn compute_checksum<T: ModelStore>(&mut self, model_store: &T) -> Result<()> {
        match self {
            Self::Unary(blob) => blob.compute_checksum(model_store),
            Self::Collection(blobs) => {
                for blob in blobs {
                    blob.compute_checksum(model_store)?
                }

                Ok(())
            }
        }
    }
}

/// BLOB in user data target with metadata.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Blob<T> {
    /// BLOB available options.
    pub kind: T,
    /// BLOB location.
    pub location: PathBuf,
    /// BLOB contents checksum.
    pub checksum: Option<String>,
    /// Where is it located
    pub store_pointer_name: Option<String>,
}

impl Blob<FileOrFolder> {
    // Function to compute the checksum based with handling for default case
    fn compute_checksum<T: ModelStore>(&mut self, model_store: &T) -> Result<()> {
        self.checksum = match &self.store_pointer_name {
            Some(store_pointer_name) => Some(
                model_store
                    .load_store_pointer(store_pointer_name)?
                    .get_store()?
                    .compute_checksum(&self.location)?,
            ),
            None => {
                Some(
                    // Empty store name, thus use default behaivor
                    model_store.compute_checksum(&self.location)?,
                )
            }
        };
        Ok(())
    }
}

/// File or folder options for BLOBs.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum FileOrFolder {
    /// A single file specified by its absolute path.
    File,
    /// A single folder specified by its absolute path.
    Folder,
}

/// Folder-only option for BLOBs.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum FolderOnly {
    /// A single folder specified by its absolute path.
    Folder,
}
