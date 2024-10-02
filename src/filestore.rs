use std::{
    fs, io,
    path::{Path, PathBuf},
};

use colored::Colorize;
use serde::{Deserialize, Serialize};

use crate::{model::Annotation, store::Store};

struct FileStore {
    storage_folder_path: PathBuf,
}

impl FileStore {
    pub fn new(storage_folder_path: PathBuf) -> Self {
        Self {
            storage_folder_path: storage_folder_path,
        }
    }

    pub fn construct_annotation_path(&self, hash: &str) -> PathBuf {
        let mut path_buf = self.storage_folder_path.to_path_buf();
        path_buf.push("Annotation");
        path_buf.push(format!("{}{}", hash, ".yaml"));

        path_buf
    }
}

/// Storage classes for yaml
#[derive(Serialize, Deserialize, Debug)]
struct AnnotationYaml {
    class: String,
    name: String,
    description: String,
    version: String,
}

impl Store for FileStore {
    fn store_annotation(
        &self,
        annotation: &crate::model::Annotation,
        hash: &str, // Of owner
    ) -> Result<(), String> {
        let data_struct = &AnnotationYaml {
            class: String::from("Annotation"),
            name: annotation.name.clone(),
            description: annotation.description.clone(),
            version: annotation.version.to_string(),
        };

        let yaml_str = serde_yaml::to_string(data_struct).expect(&format!(
            "{}{:?}",
            "Failed to seralize: ".bright_red(),
            data_struct
        ));

        let path = self.construct_annotation_path(hash);

        match create_file_and_dir_if_not_exist(&path, &yaml_str) {
            Ok(_) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    fn read_annotation(&self, hash: &str) -> Annotation {
        let mut path = self.construct_annotation_path(hash);

        let file = match fs::read_to_string(&path) {
            Ok(raw_yaml) => raw_yaml,
            Err(e) => panic!(
                "{}{}{}",
                e.to_string().bright_red(),
                " for ".bright_red(),
                path.to_string_lossy().bright_cyan()
            ),
        };

        let annotation_yaml: AnnotationYaml = match serde_yaml::from_str(&file) {
            Ok(value) => value,
            Err(e) => panic!(
                "{}{}{}{}",
                "Failed to deserialize with error ".bright_red(),
                e.to_string().bright_red(),
                " for ".bright_red(),
                path.to_string_lossy().bright_cyan()
            ),
        };

        Annotation {
            name: annotation_yaml.name,
            description: annotation_yaml.description,
            version: annotation_yaml.version,
        }
    }
    
    fn store_pod(&self, pod: &crate::model::Pod) -> Result<(), String> {
        todo!()
    }
    
    fn read_pod(&self, hash: &str) -> crate::model::Pod {
        todo!()
    }
}

/// Store Errors

fn create_file_and_dir_if_not_exist(path: &Path, content_to_write: &str) -> io::Result<()> {
    if !Path::new(&path).exists() {
        match path.parent() {
            Some(value) => fs::create_dir_all(&value)?,
            None => panic!("{}", "Unable to extract folder path".bright_red()), // Maybe do a more genric error here since std::io:result doesn't support that
        };
    }

    fs::write(&path, content_to_write)
}
