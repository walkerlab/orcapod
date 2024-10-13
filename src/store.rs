use crate::{
    error::{
        DeserializeError, FailedToExtractParentFolder, FileAlreadyExists, IOError,
    },
    model::{to_yaml, Annotation, Pod, RequiredGPU, StreamInfo},
    util::get_struct_name,
};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    path::{Path, PathBuf},
};

pub trait Store {
    fn save_pod(&self, pod: &Pod) -> Result<(), Box<dyn Error>>;
    fn load_pod(&self, name: &str, version: &str) -> Result<Pod, Box<dyn Error>>;
}

#[derive(Debug)]
pub struct LocalFileStore {
    location: PathBuf,
}

impl LocalFileStore {
    pub fn new(location: impl Into<PathBuf>) -> Self {
        Self {
            location: location.into(),
        }
    }

    fn construct_annotation_path(&self, hash: &str, ver: &str) -> PathBuf {
        let mut path_buf = self.location.clone();
        path_buf.push(get_struct_name::<Annotation>().unwrap());
        path_buf.push(hash);
        path_buf.push(format!("{}{}", ver, ".yaml"));

        path_buf
    }

    fn construct_folder_path(&self, model_name: &str, hash: &str) -> PathBuf {
        let mut path_buf = self.location.clone();
        path_buf.push(model_name);
        path_buf.push(format!("{}.yaml", hash));

        path_buf
    }
    fn store_annotation(
        &self,
        annotation: &Annotation,
        hash: &str, // Of owner
    ) -> Result<(), Box<dyn Error>> {
        create_file_and_dir_if_not_exist(
            self.construct_annotation_path(hash, &annotation.version),
            &to_yaml(annotation)?,
        )
    }

    fn load_annotation(
        &self,
        hash: &str,
        version: &str,
    ) -> Result<Annotation, Box<dyn Error>> {
        let path = self.construct_annotation_path(hash, version);

        let file = read_yaml(&path)?;

        #[derive(Deserialize)]
        struct AnnotationYaml {
            name: String,
            description: String,
            version: String,
        }

        let yaml_struct: AnnotationYaml = match serde_yaml::from_str(&file) {
            Ok(value) => value,
            Err(error) => {
                return Err(Box::new(DeserializeError { path, error }));
            }
        };

        Ok(Annotation {
            name: yaml_struct.name,
            description: yaml_struct.description,
            version: yaml_struct.version,
        })
    }
}

impl Store for LocalFileStore {
    fn save_pod(&self, pod: &Pod) -> Result<(), Box<dyn Error>> {
        let path = self.construct_folder_path(&get_struct_name::<Pod>()?, &pod.hash);

        // Try to save it
        create_file_and_dir_if_not_exist(&path, &to_yaml(&pod)?)?;

        // Missing docker image save
        // TODO

        // Save the Annotation
        self.store_annotation(&pod.annotation, &pod.hash)
    }

    fn load_pod(&self, hash: &str, version: &str) -> Result<Pod, Box<dyn Error>> {
        let path = self.construct_folder_path(&get_struct_name::<Pod>()?, hash);

        #[derive(Deserialize)]
        struct PodYaml {
            gpu: Option<RequiredGPU>,
            image: String,
            // Num of recommneded cpu cores (can be fractional)
            input_stream_map: BTreeMap<String, StreamInfo>,
            recommended_memory: u64,
            output_dir: PathBuf,
            output_stream_map: BTreeMap<String, StreamInfo>,
            recommended_cpus: f32,
            source: String, // Git Commit
            command: String,
            file_content_checksums: BTreeMap<PathBuf, String>,
        }

        let file = read_yaml(&path)?;
        let yaml_struct: PodYaml = match serde_yaml::from_str(&file) {
            Ok(value) => value,
            Err(error) => {
                return Err(Box::new(DeserializeError { path, error }));
            }
        };

        let annotation = self.load_annotation(hash, version)?;

        Ok(Pod::new(
            annotation,
            yaml_struct.source,
            yaml_struct.image,
            yaml_struct.command,
            yaml_struct.input_stream_map,
            yaml_struct.output_dir,
            yaml_struct.output_stream_map,
            yaml_struct.recommended_cpus,
            yaml_struct.recommended_memory,
            yaml_struct.gpu,
        )?)
    }
}

/// Helper Functions
///

fn create_file_and_dir_if_not_exist(
    path: impl AsRef<Path>,
    content_to_write: &str,
) -> Result<(), Box<dyn Error>> {
    if path.as_ref().exists() {
        return Err(Box::new(FileAlreadyExists {
            path: path.as_ref().into(),
        }));
    } else {
        // Create the all the folders above the file
        let parent_path = match path.as_ref().parent() {
            Some(value) => value,
            None => {
                return Err(Box::new(FailedToExtractParentFolder {
                    path: path.as_ref().into(),
                }))
            }
        };

        fs::create_dir_all(&parent_path)?;

        match fs::write(&path, content_to_write) {
            Ok(_) => Ok(()),
            Err(e) => Err(Box::new(IOError {
                path: path.as_ref().into(),
                error: e,
            })),
        }
    }
}

fn read_yaml(path: impl AsRef<Path>) -> Result<String, Box<dyn Error>> {
    Ok(match fs::read_to_string(&path) {
        Ok(raw_yaml) => raw_yaml,
        Err(error) => {
            return Err(Box::new(IOError {
                path: path.as_ref().into(),
                error,
            }))
        }
    })
}
