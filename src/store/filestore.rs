use crate::{
    error::{FileExists, FileHasNoParent, NoAnnotationFound},
    model::{from_yaml, to_yaml, Annotation, Pod},
    util::get_type_name,
};
use colored::Colorize;
use regex::Regex;
use serde::{de::DeserializeOwned, Serialize};
use std::{collections::HashSet, error::Error, fs, path::PathBuf};

use super::{ItemInfo, Store};

#[derive(Debug)]
pub struct LocalFileStore {
    pub directory: PathBuf,
}

impl Store for LocalFileStore {
    fn save_pod(&self, pod: &Pod) -> Result<(), Box<dyn Error>> {
        self.save_item(pod, &pod.annotation, &pod.hash)
    }

    fn load_pod(&self, name: &str, version: &str) -> Result<Pod, Box<dyn Error>> {
        self.load_item::<Pod>(name, version)
    }

    fn list_pod(&self) -> Result<Vec<ItemInfo>, Box<dyn Error>> {
        self.list_item::<Pod>()
    }

    fn delete_pod(&self, name: &str, version: &str) -> Result<(), Box<dyn Error>> {
        self.delete_item::<Pod>(name, version)
    }
}

impl LocalFileStore {
    pub fn new(location: impl Into<PathBuf>) -> Self {
        Self {
            directory: location.into(),
        }
    }

    // Helper Functions
    /// Create the full path for the annotation yaml file identify by hash-ver.yaml
    fn make_annotation_path<T>(&self, name: &str, hash: &str, version: &str) -> PathBuf {
        PathBuf::from(format!(
            "{}/{}/{}/{}/{}-{}.yaml",
            self.directory.to_string_lossy(),
            "annotation",
            get_type_name::<T>(),
            name,
            hash,
            version,
        ))
    }

    /// Create the full path to the spec.yaml given a hash and type T
    fn make_spec_path<T>(&self, hash: &str) -> PathBuf {
        PathBuf::from(format!(
            "{}/{}/{}/{}",
            self.directory.to_string_lossy(),
            get_type_name::<T>(),
            hash,
            "spec.yaml",
        ))
    }

    /// Given a pattern to glob against (directory query), for every valid match,
    /// perform regex on the return path splitting into 3 groups
    /// - name
    /// - hash
    /// - version
    ///
    /// Returns a Result, where the Ok is a Vec<ItemInfo>.
    ///
    /// Error is a Box<dyn Error>
    ///
    /// # Examples
    /// Given this pattern:
    /// ``` markdown
    /// /orca-data-storage/annotation/pod/example-pod/737f838cc457d833ff1dc01980aa56e9661304a26e33885defe995487e3306e7-0.0.0.yaml
    /// ```
    ///
    /// The return should be a vector of 1 assuming the path exits with the value of
    /// ``` markdown
    /// ItemInfo `{
    ///     name: example-pod,
    ///     hash: 737f838cc457d833ff1dc01980aa56e9661304a26e33885defe995487e3306e7,
    ///     version: 0.0.0
    /// }`
    /// ```
    ///
    ///
    /// Given this pattern:
    /// ``` markdown
    /// /orca-data-storage/annotation/pod/*/*-*.yaml
    /// ```
    /// The return will be in the format will n of
    /// <name, hash, version>
    /// where n is the number of unique paths matching the glob pattern
    ///
    fn search_annotation(glob_pattern: &str) -> Result<Vec<ItemInfo>, Box<dyn Error>> {
        let mut matches = Vec::new();

        let re = Regex::new(
            r"\/(?<name>[0-9a-zA-Z\- ]+)\/(?<hash>[0-9a-f]+)-(?<version>[0-9]+\.[0-9]+\.[0-9]+)\.yaml$",
        )?;

        for path in glob::glob(glob_pattern)? {
            let path_str = path?.to_string_lossy().to_string();

            let cap = match re.captures(&path_str) {
                Some(value) => value,
                None => continue,
            };

            matches.push(ItemInfo {
                name: cap["name"].into(),
                hash: cap["hash"].into(),
                version: cap["version"].into(),
            });
        }
        Ok(matches)
    }

    // Generic function for save load list delete
    /// Generic func to save all sorts of item
    ///
    /// Example usage inside LocalFileStore
    /// ``` markdown
    /// let pod = Pod::new(); // For example doesn't actually work
    /// self.save_item(pod, &pod.annotation, &pod.hash).unwrap()
    /// ```
    fn save_item<T: Serialize>(
        &self,
        item: &T,
        annotation: &Annotation,
        hash: &str,
    ) -> Result<(), Box<dyn Error>> {
        // Save the annotation file and throw and error if exist
        Self::save_file(
            &self.make_annotation_path::<T>(&annotation.name, &hash, &annotation.version),
            &serde_yaml::to_string(annotation)?,
            true,
        )?;

        // Save the pod and skip if it already exist, for the case of many annotation to a single pod
        Self::save_file(
            &self.make_spec_path::<T>(&hash),
            &to_yaml::<T>(&item)?,
            false,
        )?;

        Ok(())
    }

    /// Generic function for loading spec.yaml into memory
    fn load_item<T: DeserializeOwned>(
        &self,
        name: &str,
        version: &str,
    ) -> Result<T, Box<dyn Error>> {
        let hash = Self::search_annotation(
            &self
                .make_annotation_path::<T>(&name, "*", &version)
                .to_string_lossy(),
        )?
        .get(0)
        .ok_or(NoAnnotationFound {
            class: get_type_name::<T>(),
            name: name.to_string(),
            version: version.to_string(),
        })?
        .hash
        .clone();

        Ok(from_yaml::<T>(
            &self.make_annotation_path::<T>(&name, &hash, &version),
            &self.make_spec_path::<T>(&hash),
            &hash,
        )?)
    }

    fn list_item<T>(&self) -> Result<Vec<ItemInfo>, Box<dyn Error>> {
        Ok(Self::search_annotation(
            &self
                .make_annotation_path::<T>("*", "*", "*")
                .to_string_lossy()
                .to_owned(),
        )?)
    }

    fn delete_item<T>(&self, name: &str, version: &str) -> Result<(), Box<dyn Error>> {
        // Search for all annotation files that the matching name and version for the item
        let glob_pattern = self
            .make_annotation_path::<T>(name, "*", version)
            .to_string_lossy()
            .into_owned();

        let matches = Self::search_annotation(&glob_pattern)?;

        // If there is no matches, throw an error
        matches.is_empty().then(|| {
            return NoAnnotationFound {
                class: get_type_name::<T>(),
                name: name.into(),
                version: version.into(),
            };
        });

        // Create buffer to store the parents path to check if it is empty after all delete
        let mut annotation_parent_folder = HashSet::new();
        let mut item_hashes_to_delete = HashSet::new();

        for item in matches {
            // Delete all annotation files that matches
            let file_to_delete_path =
                self.make_annotation_path::<T>(&item.name, &item.hash, &item.version);

            // Extract parent and store it
            annotation_parent_folder.insert(
                file_to_delete_path
                    .parent()
                    .ok_or(FileHasNoParent {
                        path: file_to_delete_path.clone(),
                    })?
                    .to_path_buf(),
            );

            // Add the item hash to the vec
            item_hashes_to_delete.insert(item.hash.clone());

            fs::remove_file(self.make_annotation_path::<T>(&item.name, &item.hash, &item.version))?;
        }

        // Go through and check if the parent folder is empty, if so delete it too
        for parent_folder in annotation_parent_folder.iter() {
            if parent_folder.read_dir()?.next().is_none() {
                fs::remove_dir(parent_folder)?;
            }
        }

        // Delete the item, but check if there is no other annotation referencing it
        for item_hash in item_hashes_to_delete.iter() {
            // Search annotation to see if there something pointing to it
            let search_pattern = self
                .make_annotation_path::<T>("*", &item_hash, "*")
                .to_string_lossy()
                .into_owned();
            let matches = Self::search_annotation(&search_pattern)?;

            if matches.is_empty() {
                // Okay to delete as no other annotation is pointing to it
                let spec_path = self.make_spec_path::<T>(&item_hash);
                fs::remove_dir_all(spec_path.parent().ok_or(FileHasNoParent {
                    path: spec_path.clone(),
                })?)?;
            }
        }

        Ok(())
    }

    // Help save file function
    fn save_file(
        file: &PathBuf,
        content: &str,
        fail_if_exists: bool,
    ) -> Result<(), Box<dyn Error>> {
        fs::create_dir_all(
            &file
                .parent()
                .ok_or(FileHasNoParent { path: file.clone() })?,
        )?;
        let file_exists = fs::exists(&file)?;
        if file_exists {
            if !fail_if_exists {
                println!(
                    "Skip saving `{}` since it is already stored.",
                    file.to_string_lossy().bright_cyan(),
                );
                return Ok(());
            } else {
                return Err(Box::new(FileExists { path: file.clone() }));
            }
        } else {
            fs::write(&file, content)?;
        }
        Ok(())
    }
}
