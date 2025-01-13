use crate::{
    crypto::{hash_buf_reader, hash_bytes, HASH_SIZE_IN_BYTES},
    error::{Kind, OrcaError, Result},
    model::{to_yaml, Annotation, Pod, PodJob, PodResult},
    store::{ModelID, ModelInfo, ModelStore},
    util::get_type_name,
};
use colored::Colorize;
use regex::Regex;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_yaml::Value;
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File},
    io::BufReader,
    path::{Path, PathBuf},
};

use super::{DataStore, StorePointer};

/// Relative path where model specification is stored within the model directory.
static SPEC_FILENAME: &str = "spec.yaml";
/// How large should the read file buffer be (for chunk reading during check sum computing)
static BUFFER_READER_CAP: usize = 2 << 13; // 8KB chunks to match with page size typically found

/// Support for a storage backend on a local filesystem directory.
#[derive(Debug, Serialize, Deserialize)]
pub struct LocalStore {
    /// A local path to a directory where store will be located.
    directory: PathBuf,
}

impl LocalStore {
    /// Construct a local file store instance in a specific directory.
    pub fn new(directory: impl AsRef<Path>) -> Self {
        Self {
            directory: directory.as_ref().into(),
        }
    }

    /// Relative path where model annotation is stored within the model directory.
    pub fn make_annotation_relpath(name: &str, version: &str) -> PathBuf {
        PathBuf::from(format!("annotation/{name}-{version}.yaml"))
    }

    /// Makes the path to model type
    pub fn make_model_path<T>(&self) -> PathBuf {
        PathBuf::from(format!(
            "{}/model/{}",
            self.directory.to_string_lossy(),
            get_type_name::<T>()
        ))
    }

    /// Makes the path to the data store
    pub fn make_data_path(&self) -> PathBuf {
        PathBuf::from(format!("{}/data/", self.directory.to_string_lossy(),))
    }

    /// Makes path to the hash object
    pub fn make_hash_path<T>(&self, hash: impl AsRef<Path>) -> PathBuf {
        self.make_model_path::<T>().join(hash)
    }

    /// Build the storage path with the model directory (`hash`) and a file's relative path.
    pub fn make_hash_rel_path<T>(&self, hash: &str, relpath: impl AsRef<Path>) -> PathBuf {
        self.make_hash_path::<T>(hash).join(relpath)
    }

    fn find_annotation(glob_pattern: &Path) -> Result<impl Iterator<Item = Result<ModelInfo>>> {
        let re = Regex::new(
            r"^.*\/(?<hash>[0-9a-f]+)\/annotation\/(?<name>[0-9a-zA-Z\- ]+)-(?<version>[0-9]+\.[0-9]+\.[0-9]+)\.yaml$",
        )?;
        let paths = glob::glob(&glob_pattern.to_string_lossy())?.map(move |filepath| {
            let filepath_string = String::from(filepath?.to_string_lossy());
            let group = re
                .captures(&filepath_string)
                .ok_or_else(|| OrcaError::from(Kind::NoRegexMatch))?;

            Ok(ModelInfo {
                name: group["name"].to_string(),
                version: group["version"].to_string(),
                hash: group["hash"].to_string(),
            })
        });
        Ok(paths)
    }

    fn get_key_from_yaml(path: impl AsRef<Path>, key: &str) -> Result<String> {
        // Target Yaml
        let target_yaml = fs::read_to_string(path)?;

        // Pull out pod hash from yaml
        let yaml_mapping: BTreeMap<String, Value> = serde_yaml::from_str(&target_yaml)?;
        let value = yaml_mapping.get(key).ok_or_else(|| {
            OrcaError::from(Kind::MissingPodHashFromPodJobYaml(target_yaml.clone()))
        })?;

        Ok(value
            .as_str()
            .ok_or_else(|| OrcaError::from(Kind::FailedToCovertValueToString))?
            .to_owned())
    }

    fn lookup_hash<T>(
        &self,
        name_match_pattern: &str,
        version_match_pattern: &str,
    ) -> Result<String> {
        let matches = Self::find_annotation(&self.make_hash_rel_path::<T>(
            "*",
            &Self::make_annotation_relpath(name_match_pattern, version_match_pattern),
        ))?
        .map(|item| Ok::<String, OrcaError>(item?.hash))
        .collect::<Result<Vec<String>>>()?;

        if matches.len() != 1 {
            return Err(OrcaError::from(Kind::MultipleHashesForAnnotation(
                name_match_pattern.to_owned(),
                version_match_pattern.to_owned(),
            )));
        }

        matches
            .first()
            .ok_or_else(|| OrcaError::from(Kind::InvalidIndex(0)))
            .cloned()
    }

    fn save_file_internal(
        file: impl AsRef<Path>,
        content: impl AsRef<[u8]>,
        fail_if_exists: bool,
    ) -> Result<()> {
        if let Some(parent) = file.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        let file_exists = file.as_ref().exists();
        if file_exists && fail_if_exists {
            return Err(OrcaError::from(Kind::FileExists(
                file.as_ref().to_path_buf(),
            )));
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
        annotation: &Option<Annotation>,
    ) -> Result<()> {
        // Save the annotation file and throw and error if exist if exist
        if let Some(annotation_unwrap) = annotation {
            Self::save_file_internal(
                self.make_hash_rel_path::<T>(
                    hash,
                    &Self::make_annotation_relpath(
                        &annotation_unwrap.name,
                        &annotation_unwrap.version,
                    ),
                ),
                &serde_yaml::to_string(&annotation_unwrap)?,
                true,
            )?;
        }

        // Save the pod and skip if it already exist, for the case of many annotation to a single pod
        Self::save_file_internal(
            self.make_hash_rel_path::<T>(hash, SPEC_FILENAME),
            to_yaml(model)?,
            false,
        )?;

        Ok(())
    }

    fn load_model<T: DeserializeOwned>(&self, hash: &str) -> Result<T> {
        Ok(serde_yaml::from_str(&fs::read_to_string(
            self.make_hash_rel_path::<T>(hash, SPEC_FILENAME),
        )?)?)
    }

    fn get_hash_and_annotation<T>(
        &self,
        model_id: &ModelID,
    ) -> Result<(String, Option<Annotation>)> {
        match model_id {
            ModelID::Hash(hash) => Ok((hash.clone(), None)),
            ModelID::Annotation(name, version) => {
                // Deal with the hash
                let hash = self.lookup_hash::<T>(name, version)?;

                // Deal with the annotation
                let annotation: Annotation =
                    serde_yaml::from_str(&fs::read_to_string(self.make_hash_rel_path::<T>(
                        &hash,
                        Self::make_annotation_relpath(name, version),
                    ))?)?;
                Ok((hash, Some(annotation)))
            }
        }
    }

    fn list_model<T>(&self) -> Result<Vec<ModelInfo>> {
        // Get all hashes first and store them in the btreemap
        let mut unique_hashes = HashSet::new();

        let hash_paths = glob::glob(&self.make_model_path::<T>().join("*").to_string_lossy())?;
        let re = Regex::new(r"^.*\/(?<hash>[0-9a-f]+)$")?;

        for path in hash_paths {
            let path_string = String::from(path?.to_string_lossy());
            let captures = re
                .captures(&path_string)
                .ok_or_else(|| OrcaError::from(Kind::NoRegexMatch))?;

            unique_hashes.insert(captures["hash"].to_owned());
        }

        // Get the vector of model info
        let mut model_infos_with_annotation = Self::find_annotation(
            &self.make_hash_rel_path::<T>("*", &Self::make_annotation_relpath("*", "*")),
        )?
        .collect::<Result<Vec<ModelInfo>>>()?;

        // Go through all the model info and remove hash from unique hashes
        for model_info in &model_infos_with_annotation {
            unique_hashes.remove(&model_info.hash);
        }

        // Insert whatever remains with empty fields for name and version
        for annotationless_hash in &unique_hashes {
            model_infos_with_annotation.push(ModelInfo {
                name: String::new(),
                version: String::new(),
                hash: annotationless_hash.to_owned(),
            });
        }

        Ok(model_infos_with_annotation)
    }

    fn delete_model<T>(&self, model_id: &ModelID) -> Result<()> {
        // assumes propagate = false
        let hash = match model_id {
            ModelID::Hash(hash) => hash,
            ModelID::Annotation(name, version) => &self.lookup_hash::<T>(name, version)?,
        };
        let spec_dir = self.make_hash_rel_path::<T>(hash, "");
        fs::remove_dir_all(spec_dir)?;

        Ok(())
    }
}

impl DataStore for LocalStore {
    fn from_uri(uri: &str) -> Result<Self> {
        // Remove the class name from the start
        let directory = uri.split("::").collect::<Vec<&str>>()[1];
        if !PathBuf::from(directory).exists() {
            // uri is not valid
            return Err(OrcaError::from(Kind::InvalidURIForFileStore(
                "Directory doesn't exist or not accessible".to_owned(),
                directory.to_owned(),
            )));
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

    fn compute_checksum_for_path(&self, path: impl AsRef<Path>) -> Result<String> {
        let full_path = self.make_data_path().join(path.as_ref());

        if !full_path.exists() {
            return Err(OrcaError::from(Kind::PathDoesNotExist(full_path)));
        }

        if full_path.is_file() {
            // Read and hash in chunks
            let file = File::open(full_path)?;
            let buf_reader = BufReader::with_capacity(BUFFER_READER_CAP, file);

            // Use the buf_reader hashing
            hash_buf_reader(buf_reader)
        } else if full_path.is_dir() {
            // Path is a directory, thus we will need to recursively hash and sort
            let hashes = path
                .as_ref()
                .read_dir()?
                .map(|dir_entry| Ok((self.compute_checksum_for_path(dir_entry?.path())?, ())))
                .collect::<Result<BTreeMap<String, ()>>>()?;

            // Combine all the hashes by alpha numeric order
            let mut hashes_buffer = String::with_capacity(HASH_SIZE_IN_BYTES);
            for hash in hashes.keys() {
                hashes_buffer.push_str(hash);
            }

            // Hash the buffer
            Ok(hash_bytes(hashes_buffer))
        } else {
            // Unknown type of path or unsupported, thus panic for now
            Err(OrcaError::from(Kind::UnsupportedPath(full_path)))
        }
    }

    fn load_file(&self, path: impl AsRef<Path>) -> Result<Vec<u8>> {
        Ok(fs::read(self.make_data_path().join(path))?)
    }

    fn save_file(&self, path: impl AsRef<Path>, content: Vec<u8>) -> Result<()> {
        Self::save_file_internal(self.make_data_path().join(path), content, true)
    }
}

impl ModelStore for LocalStore {
    fn delete_annotation<T>(&self, name: &str, version: &str) -> Result<()> {
        if get_type_name::<T>() == "store_pointer" {
            return Err(OrcaError::from(
                Kind::DeletingAnnotationForStorePointerNotAllowed,
            ));
        }

        let hash = self.lookup_hash::<T>(name, version)?;

        let annotation_file =
            self.make_hash_rel_path::<T>(&hash, &Self::make_annotation_relpath(name, version));
        fs::remove_file(&annotation_file)?;

        Ok(())
    }

    fn save_pod(&self, pod: &Pod) -> Result<()> {
        self.save_model(pod, &pod.hash, &pod.annotation)
    }

    fn load_pod(&self, model_id: &ModelID) -> Result<Pod> {
        let (hash, annotation) = self.get_hash_and_annotation::<Pod>(model_id)?;

        let mut pod: Pod = self.load_model(&hash)?;
        pod.hash = hash;
        pod.annotation = annotation;

        Ok(pod)
    }

    fn list_pod(&self) -> Result<Vec<ModelInfo>> {
        self.list_model::<Pod>()
    }

    fn delete_pod(&self, model_id: &ModelID) -> Result<()> {
        self.delete_model::<Pod>(model_id)
    }

    fn save_pod_job(&self, pod_job: &mut PodJob) -> Result<()> {
        pod_job.compute_checksum_for_input(self)?;
        self.save_pod(&pod_job.pod)?;
        self.save_model(pod_job, &pod_job.hash, &pod_job.annotation)
    }

    fn load_pod_job(&self, model_id: &ModelID) -> Result<PodJob> {
        let (hash, annotation) = self.get_hash_and_annotation::<PodJob>(model_id)?;

        let mut pod_job = self.load_model::<PodJob>(&hash)?;
        pod_job.hash = hash;
        pod_job.annotation = annotation;
        // Load annotation first if model_id was type annotation

        // Get the pod
        pod_job.pod = self.load_pod(&ModelID::Hash(Self::get_key_from_yaml(
            self.make_hash_rel_path::<PodJob>(&pod_job.hash, SPEC_FILENAME),
            "pod_hash",
        )?))?;

        Ok(pod_job)
    }

    fn list_pod_job(&self) -> Result<Vec<ModelInfo>> {
        self.list_model::<PodJob>()
    }

    fn delete_pod_job(&self, model_id: &ModelID) -> Result<()> {
        self.delete_model::<PodJob>(model_id)
    }

    fn save_store_pointer(&self, store_pointer: &StorePointer) -> Result<()> {
        self.save_model(
            store_pointer,
            &store_pointer.hash,
            &Some(store_pointer.annotation.clone()),
        )
    }

    fn load_store_pointer(&self, store_name: &str) -> Result<StorePointer> {
        // Search all the annotations in store_pointer to

        let glob_pattern = self.make_hash_rel_path::<StorePointer>(
            "*",
            Self::make_annotation_relpath(store_name, "*"),
        );

        let mut model_infos =
            Self::find_annotation(&glob_pattern)?.collect::<Result<Vec<ModelInfo>>>()?;

        // Sort the versions in ascending order
        model_infos.sort_by(|a, b| a.version.cmp(&b.version));

        // Get the lastest version
        let latest_model_info = model_infos.last().ok_or_else(|| {
            OrcaError::from(Kind::NoAnnotationFound(
                get_type_name::<StorePointer>(),
                store_name.to_owned(),
                "*".to_owned(),
            ))
        })?;

        // Load the latest store pointer
        let (hash, annotation) =
            self.get_hash_and_annotation::<StorePointer>(&ModelID::Annotation(
                latest_model_info.name.clone(),
                latest_model_info.version.clone(),
            ))?;

        let mut store_pointer = self.load_model::<StorePointer>(&latest_model_info.hash)?;
        store_pointer.hash = hash;
        store_pointer.annotation = annotation.ok_or_else(|| {
            OrcaError::from(Kind::NoAnnotationFound(
                get_type_name::<StorePointer>(),
                store_name.to_owned(),
                "*".to_owned(),
            ))
        })?;

        Ok(store_pointer)
    }

    fn list_store_pointer(&self) -> Result<Vec<ModelInfo>> {
        self.list_model::<StorePointer>()
    }

    fn delete_store_pointer(&self, model_id: &ModelID) -> Result<()> {
        self.delete_model::<StorePointer>(model_id)
    }

    fn wipe(&self) -> Result<()> {
        Ok(fs::remove_dir_all(&self.directory)?)
    }

    fn save_pod_result(&self, pod_result: &PodResult) -> Result<()> {
        self.save_model(pod_result, &pod_result.hash, &None)
    }

    fn load_pod_result(&self, hash: &str) -> Result<PodResult> {
        let mut pod_result = self.load_model::<PodResult>(hash)?;

        hash.clone_into(&mut pod_result.hash);

        // Load the pod job
        pod_result.pod_job = self.load_pod_job(&ModelID::Hash(Self::get_key_from_yaml(
            self.make_hash_rel_path::<PodResult>(hash, SPEC_FILENAME),
            "pod_job_hash",
        )?))?;

        Ok(pod_result)
    }

    fn list_pod_result(&self) -> Result<Vec<ModelInfo>> {
        self.list_model::<PodResult>()
    }

    fn delete_pod_result(&self, hash: &str) -> Result<()> {
        self.delete_model::<PodResult>(&ModelID::Hash(hash.to_owned()))
    }
}
