use crate::{
    crypto::{hash_buf_reader, hash_bytes},
    error::{Kind, OrcaError, Result},
    model::{to_yaml, Annotation, Pod, PodJob, StorePointer},
    store::{ModelID, ModelInfo},
    util::get_type_name,
};
use colored::Colorize;
use glob::glob;
use regex::Regex;
use serde::{de::DeserializeOwned, Serialize};
use std::fs;
use std::{
    collections::BTreeMap,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

use super::{DataStore, ModelStore};
static BUFFER_READER_CAP: usize = 2 << 13; // 8KB chunks to match with page size typically found

/// Relative path where model specification is stored within the model directory.
pub const SPEC_RELPATH: &str = "spec.yaml";
/// Support for a storage backend on a local filesystem directory.
#[derive(Debug)]
pub struct LocalFileStore {
    /// A local path to a directory where store will be located.
    directory: PathBuf,
}

impl ModelStore for LocalFileStore {
    fn save_pod(&self, pod: &Pod) -> Result<()> {
        self.save_model(pod, &pod.hash, pod.annotation.as_ref())
    }

    fn load_pod(&self, model_id: &ModelID) -> Result<Pod> {
        let (mut pod, annotation, hash) = self.load_model::<Pod>(model_id)?;
        pod.annotation = annotation;
        pod.hash = hash;
        Ok(pod)
    }

    fn list_pod(&self) -> Result<Vec<ModelInfo>> {
        self.list_model::<Pod>()
    }

    fn delete_pod(&self, model_id: &ModelID) -> Result<()> {
        self.delete_model::<Pod>(model_id)
    }

    fn save_pod_job(&self, pod_job: &mut PodJob) -> Result<()> {
        pod_job.compute_checksum_for_input_stream_path(self)?;
        self.save_pod(&pod_job.pod)?;
        self.save_model(pod_job, &pod_job.hash, pod_job.annotation.as_ref())
    }

    fn load_pod_job(&self, model_id: &ModelID) -> Result<PodJob> {
        let (mut pod_job, annotation, hash) = self.load_model::<PodJob>(model_id)?;
        pod_job.annotation = annotation;
        pod_job.hash = hash;
        pod_job.pod = self.load_pod(&ModelID::Hash(pod_job.pod.hash))?;
        Ok(pod_job)
    }

    fn list_pod_job(&self) -> Result<Vec<ModelInfo>> {
        self.list_model::<PodJob>()
    }

    fn delete_pod_job(&self, model_id: &ModelID) -> Result<()> {
        self.delete_model::<PodJob>(model_id)
    }

    fn delete_annotation<T>(&self, name: &str, version: &str) -> Result<()> {
        let hash = self.lookup_hash::<T>(name, version)?;
        let annotation_file =
            self.make_path::<T>(&hash, Self::make_annotation_relpath(name, version));
        fs::remove_file(&annotation_file)?;

        Ok(())
    }
    fn save_store_pointer(&self, store_pointer: &StorePointer) -> Result<()> {
        self.save_model(
            store_pointer,
            &store_pointer.hash,
            Some(&store_pointer.annotation),
        )
    }

    fn load_store_pointer(&self, store_name: &str) -> Result<StorePointer> {
        // Search all the annotations in store_pointer to

        let glob_pattern = self.make_path::<StorePointer>("*", "annotation/*");

        let mut model_infos = Self::find_model_metadata(&glob_pattern)?;

        // Sort the versions in ascending order
        model_infos.sort_by(|a, b| a.version.cmp(&b.version));

        // Get the lastest version
        let latest_model_info = model_infos.last().ok_or_else(|| {
            OrcaError::from(Kind::NoAnnotationFound {
                class: get_type_name::<StorePointer>(),
                name: store_name.to_owned(),
                version: "*".to_owned(),
            })
        })?;

        let name = latest_model_info.name.clone().ok_or_else(|| {
            OrcaError::from(Kind::NoAnnotationFound {
                class: get_type_name::<StorePointer>(),
                name: store_name.to_owned(),
                version: "*".to_owned(),
            })
        })?;

        let version = latest_model_info.version.clone().ok_or_else(|| {
            OrcaError::from(Kind::NoAnnotationFound {
                class: get_type_name::<StorePointer>(),
                name: store_name.to_owned(),
                version: "*".to_owned(),
            })
        })?;

        let (mut store_pointer, annotation, hash) =
            self.load_model::<StorePointer>(&ModelID::Annotation(name, version))?;

        store_pointer.annotation = annotation.ok_or_else(|| {
            OrcaError::from(Kind::NoAnnotationFound {
                class: get_type_name::<StorePointer>(),
                name: store_name.to_owned(),
                version: "*".to_owned(),
            })
        })?;
        store_pointer.hash = hash;

        Ok(store_pointer)
    }

    fn list_store_pointer(&self) -> Result<Vec<ModelInfo>> {
        self.list_model::<StorePointer>()
    }

    fn delete_store_pointer(&self, model_id: &ModelID) -> Result<()> {
        self.delete_model::<StorePointer>(model_id)
    }
}

impl DataStore for LocalFileStore {
    fn compute_checksum(&self, path: &dyn AsRef<Path>) -> Result<String> {
        let full_path = self.make_data_path().join(path.as_ref());

        if !full_path.exists() {
            return Err(OrcaError::from(Kind::PathDoesNotExist { path: full_path }));
        }

        if full_path.is_file() {
            // Read and hash in chunks
            let buf_reader = BufReader::with_capacity(BUFFER_READER_CAP, File::open(full_path)?);

            // Use the buf_reader hashing
            hash_buf_reader(buf_reader)
        } else if full_path.is_dir() {
            // Path is a directory, thus we will need to recursively hash and sort
            let hashes = path
                .as_ref()
                .read_dir()?
                .map(|dir_entry| Ok((self.compute_checksum(&dir_entry?.path())?, ())))
                .collect::<Result<BTreeMap<String, ()>>>()?;

            // Combine all the hashes by alpha numeric order
            let mut hashes_buffer = String::new();
            for hash in hashes.keys() {
                hashes_buffer.push_str(hash);
            }

            // Hash the buffer
            Ok(hash_bytes(hashes_buffer))
        } else {
            // Unknown type of path or unsupported, thus panic for now
            Err(OrcaError::from(Kind::UnsupportedPath {
                path: path.as_ref().to_path_buf(),
            }))
        }
    }

    fn from_uri(uri: &str) -> Result<Self> {
        // Remove the class name from the start
        let directory = uri.split("::").collect::<Vec<&str>>()[1];
        if !PathBuf::from(directory).exists() {
            // uri is not valid
            return Err(OrcaError::from(Kind::InvalidURIForFileStore {
                err_msg: format!(
                    "Directory {} doesn't exist or not accessible ",
                    directory.to_owned()
                ),
            }));
        }

        Ok(Self {
            directory: directory.into(),
        })
    }

    fn get_uri(&self) -> String {
        let mut uri = String::from("LocalStore::");
        uri.push_str(&self.directory.to_string_lossy());
        uri
    }
}

impl LocalFileStore {
    /// Get the directory where store is located.
    pub fn get_directory(&self) -> &Path {
        &self.directory
    }
    /// Construct a local file store instance in a specific directory.
    pub fn new(directory: impl AsRef<Path>) -> Self {
        Self {
            directory: directory.as_ref().into(),
        }
    }

    fn make_data_path(&self) -> PathBuf {
        self.directory.join(Self::DEFAULT_DATA_NAMESPACE)
    }

    /// Relative path where model annotation is stored within the model directory.
    pub fn make_annotation_relpath(name: &str, version: &str) -> PathBuf {
        PathBuf::from(format!("annotation/{name}-{version}.yaml"))
    }
    /// Build the storage path with the model directory (`hash`) and a file's relative path.
    pub fn make_path<T>(&self, hash: &str, relpath: impl AsRef<Path>) -> PathBuf {
        PathBuf::from(format!(
            "{}/{}/{}/{}",
            self.directory.to_string_lossy(),
            Self::MODEL_NAMESPACE,
            get_type_name::<T>(),
            hash
        ))
        .join(relpath)
    }

    fn find_model_metadata(glob_pattern: &Path) -> Result<Vec<ModelInfo>> {
        let re = Regex::new(
            r"(?x)
            ^
                (?<store_directory>.*)\/
                    (?<namespace>[a-z_]+)\/
                        (?<class>[a-z_]+)\/
                            (?<hash>[0-9a-f]+)\/
                                (
                                    annotation\/
                                        (?<name>[0-9a-zA-Z\-\s]+)
                                        -
                                        (?<version>[0-9]+\.[0-9]+\.[0-9]+)
                                        \.yaml
                                |
                                    spec\.yaml
                                )
            $
            ",
        )?;

        Ok(glob(&glob_pattern.to_string_lossy())?
            .filter_map(move |filepath| {
                let filepath_string = String::from(filepath.ok()?.to_string_lossy());
                let group = re.captures(&filepath_string)?;
                Some(ModelInfo {
                    name: group.name("name").map(|name| name.as_str().to_owned()),
                    version: group
                        .name("version")
                        .map(|version| version.as_str().to_owned()),
                    hash: group["hash"].to_string(),
                })
            })
            .collect())
    }

    fn lookup_hash<T>(&self, name: &str, version: &str) -> Result<String> {
        let model_infos = Self::find_model_metadata(
            &self.make_path::<T>("*", Self::make_annotation_relpath(name, version)),
        )?;

        if model_infos.len() > 1 {
            return Err(OrcaError::from(Kind::MultipleHashFound {
                name: name.to_owned(),
                ver: version.to_owned(),
            }));
        }

        Ok(model_infos
            .first()
            .ok_or_else(|| {
                OrcaError::from(Kind::NoAnnotationFound {
                    class: get_type_name::<T>(),
                    name: name.to_owned(),
                    version: version.to_owned(),
                })
            })?
            .hash
            .clone())
    }

    fn save_file(
        file: impl AsRef<Path>,
        content: impl AsRef<[u8]>,
        fail_if_exists: bool,
    ) -> Result<()> {
        if let Some(parent) = file.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        let file_exists = file.as_ref().exists();
        if file_exists && fail_if_exists {
            return Err(OrcaError::from(Kind::FileExists {
                path: file.as_ref().to_path_buf(),
            }));
        } else if file_exists {
            println!(
                "Skip saving `{}` since it is already stored.",
                file.as_ref().to_string_lossy().bright_cyan(),
            );
        } else {
            fs::write(file, content)?;
        }
        Ok(())
    }

    fn save_model<T: Serialize>(
        &self,
        model: &T,
        hash: &str,
        annotation: Option<&Annotation>,
    ) -> Result<()> {
        if let Some(provided_annotation) = annotation {
            // Save the annotation file and throw an error if exist
            Self::save_file(
                self.make_path::<T>(
                    hash,
                    Self::make_annotation_relpath(
                        &provided_annotation.name,
                        &provided_annotation.version,
                    ),
                ),
                serde_yaml::to_string(provided_annotation)?,
                true,
            )?;
        }
        // Save the pod and skip if it already exist, for the case of many annotation to a single pod
        Self::save_file(
            self.make_path::<T>(hash, SPEC_RELPATH),
            to_yaml(model)?,
            false,
        )?;

        Ok(())
    }

    fn load_model<T: DeserializeOwned>(
        &self,
        model_id: &ModelID,
    ) -> Result<(T, Option<Annotation>, String)> {
        match model_id {
            ModelID::Hash(hash) => Ok((
                serde_yaml::from_str(&fs::read_to_string(
                    self.make_path::<T>(hash, SPEC_RELPATH),
                )?)?,
                None,
                hash.clone(),
            )),
            ModelID::Annotation(name, version) => {
                let hash = self.lookup_hash::<T>(name, version)?;
                Ok((
                    serde_yaml::from_str(&fs::read_to_string(
                        self.make_path::<T>(&hash, SPEC_RELPATH),
                    )?)?,
                    serde_yaml::from_str(&fs::read_to_string(
                        self.make_path::<T>(&hash, &Self::make_annotation_relpath(name, version)),
                    )?)?,
                    hash,
                ))
            }
        }
    }

    fn list_model<T>(&self) -> Result<Vec<ModelInfo>> {
        Self::find_model_metadata(&self.make_path::<T>("**", "*"))
    }

    fn delete_model<T>(&self, model_id: &ModelID) -> Result<()> {
        // assumes propagate = false
        let hash = match model_id {
            ModelID::Hash(hash) => hash,
            ModelID::Annotation(name, version) => &self.lookup_hash::<T>(name, version)?,
        };
        let spec_dir = self.make_path::<T>(hash, "");
        fs::remove_dir_all(spec_dir)?;

        Ok(())
    }
}
