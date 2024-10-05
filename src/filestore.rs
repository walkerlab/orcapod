use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use colored::Colorize;
use serde::Deserialize;

use crate::{
    model::{Annotation, GPUSpecRequirement, KeyInfo, Pod, ToYaml},
    store::Store,
};

static POD_FOLDER_NAME: &str = "Pod";

pub struct FileStore {
    storage_folder_path: PathBuf,
}

impl FileStore {
    pub fn new(storage_folder_path: impl Into<PathBuf>) -> Self {
        Self {
            storage_folder_path: storage_folder_path.into(),
        }
    }

    fn construct_annotation_path(&self, hash: &str) -> PathBuf {
        let mut path_buf = self.storage_folder_path.clone();
        path_buf.push("Annotation");
        path_buf.push(format!("{}{}", hash, ".yaml"));

        path_buf
    }

    fn construct_folder_path(&self, model_name: &str, hash: &str) -> PathBuf {
        let mut path_buf = self.storage_folder_path.clone();
        path_buf.push(model_name);
        path_buf.push(format!("{}.yaml", hash));

        path_buf
    }
}

impl Store for FileStore {
    fn store_annotation(
        &self,
        annotation: &Annotation,
        hash: &str, // Of owner
    ) -> Result<(), String> {
        let yaml_str = annotation.to_yaml();

        let path = self.construct_annotation_path(hash);
        create_file_and_dir_if_not_exist(&path, &yaml_str)
    }

    fn load_annotation(&self, hash: &str) -> Result<Annotation, String> {
        let path = self.construct_annotation_path(hash);

        let file = read_yaml(&path)?;

        #[derive(Deserialize)]
        struct AnnotationYaml {
            name: String,
            description: String,
            version: String,
        }

        let yaml_struct: AnnotationYaml = match serde_yaml::from_str(&file) {
            Ok(value) => value,
            Err(e) => {
                return Err(format!(
                    "{}{}{}{}",
                    "Failed to deserialize with error ".bright_red(),
                    e.to_string().bright_red(),
                    " for ".bright_red(),
                    path.to_string_lossy().bright_cyan()
                ))
            }
        };

        Ok(Annotation {
            name: yaml_struct.name,
            description: yaml_struct.description,
            version: yaml_struct.version,
        })
    }

    fn store_pod(&self, pod: &Pod) -> Result<(), String> {
        let path = self.construct_folder_path(POD_FOLDER_NAME, &pod.pod_hash);

        // Try to save it
        match create_file_and_dir_if_not_exist(&path, &pod.to_yaml()) {
            Ok(_) => (),
            Err(e) => return Err(e.to_string()),
        };

        // Missing docker image save
        // TODO

        // Save the Annotation
        match self.store_annotation(&pod.annotation, &pod.pod_hash) {
            Ok(_) => Ok(()),
            Err(e) => {
                // Failed to store thus reverse the pod save
                fs::remove_dir(path).unwrap();
                Err(e.to_string())
            }
        }
    }

    fn load_pod(&self, hash: &str) -> Result<Pod, String> {
        let path = self.construct_folder_path(POD_FOLDER_NAME, hash);

        #[derive(Deserialize)]
        struct PodYaml {
            gpu: Option<GPUSpecRequirement>,
            image_digest: String,
            input_stream_map: BTreeMap<String, KeyInfo>, // Num of recommneded cpu cores (can be fractional)
            min_memory: u64,
            output_dir: PathBuf,
            output_stream_map: BTreeMap<String, KeyInfo>,
            recommended_cpus: f32,
            source_commit: String, // Git Commit
        }

        let file = read_yaml(&path)?;
        let yaml_struct: PodYaml = match serde_yaml::from_str(&file) {
            Ok(value) => value,
            Err(e) => {
                return Err(format!(
                    "{}{}{}{}",
                    "Failed to deserialize with error ".bright_red(),
                    e.to_string().bright_red(),
                    " for ".bright_red(),
                    path.to_string_lossy().bright_cyan()
                ))
            }
        };

        let annotation = self.load_annotation(hash)?;

        let pod = Pod {
            annotation,
            gpu_spec_requirments: yaml_struct.gpu,
            image_digest: yaml_struct.image_digest,
            input_stream_map: yaml_struct.input_stream_map,
            min_memory: yaml_struct.min_memory,
            output_dir: yaml_struct.output_dir,
            output_stream_map: yaml_struct.output_stream_map,
            pod_hash: hash.to_string(),
            recommended_cpus: yaml_struct.recommended_cpus,
            source_commit: yaml_struct.source_commit,
        };

        pod.verify()?;
        Ok(pod)
    }
}

/// Helper Functions
///

fn create_file_and_dir_if_not_exist(path: &Path, content_to_write: &str) -> Result<(), String> {
    if Path::new(&path).exists() {
        Err(format!(
            "{}{}",
            &path.to_string_lossy().bright_cyan(),
            " already exists!".bright_red()
        ))
    } else {
        // Create the all the folders above the file
        let parent_path = match path.parent() {
            Some(value) => value,
            None => panic!("{}", "Unable to extract folder path".bright_red()), // Maybe do a more genric error here since std::io:result doesn't support that
        };

        match fs::create_dir_all(&parent_path) {
            Ok(_) => (),
            Err(e) => {
                return Err(format!(
                    "{}{}{}{}",
                    "Failed to create parent directory ".bright_red(),
                    &parent_path.to_string_lossy().bright_cyan(),
                    " with error ".bright_red(),
                    e.to_string().bright_cyan()
                ))
            }
        }

        match fs::write(&path, content_to_write) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!(
                "{}{}{}{}",
                "Failed to write to: ".bright_red(),
                &path.to_string_lossy().cyan(),
                " with error ".bright_red(),
                e.to_string().bright_cyan()
            )),
        }
    }
}

fn read_yaml(path: &Path) -> Result<String, String> {
    Ok(match fs::read_to_string(&path) {
        Ok(raw_yaml) => raw_yaml,
        Err(e) => {
            return Err(format!(
                "{}{}{}",
                e.to_string().bright_red(),
                " for ".bright_red(),
                path.to_string_lossy().bright_cyan(),
            ))
        }
    })
}
