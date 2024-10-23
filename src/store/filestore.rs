use crate::{
    error::{FileExists, FileHasNoParent, KeyMissingFromBTree, NoAnnotationFound},
    model::{from_yaml, to_yaml, Annotation, Pod},
    util::get_type_name,
};
use anyhow::Result;
use colored::Colorize;
use regex::Regex;
use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use super::Store;

#[derive(Debug)]
pub struct LocalFileStore {
    pub directory: PathBuf,
    // Where BTreeString<struct_type, BTreeMap<name-version, hash>
    name_ver_cache: BTreeMap<String, BTreeMap<String, String>>,
}

impl Store for LocalFileStore {
    fn save_pod(&mut self, pod: &Pod) -> Result<()> {
        self.save_item(pod, &pod.hash, pod.annotation.as_ref())
    }

    fn load_pod(&mut self, name: &str, version: &str) -> Result<Pod> {
        self.load_item::<Pod>(name, version)
    }

    /// Return the name version index btree where key is name-version and value is hash of pod
    fn list_pod(&mut self) -> Result<&BTreeMap<String, String>> {
        self.build_cache_if_not_exist::<Pod>()?;
        Ok(self
            .name_ver_cache
            .get(&get_type_name::<Pod>())
            .ok_or_else(|| KeyMissingFromBTree {
                key: get_type_name::<Pod>(),
            })?)
    }

    fn delete_pod(&mut self, name: &str, version: &str) -> Result<()> {
        self.delete_item::<Pod>(name, version)
    }

    fn delete_annotation<T>(&mut self, name: &str, version: &str) -> Result<()> {
        // Search the name ver index for the hash
        let hash = self.get_hash_from_cache::<T>(name, version)?;

        fs::remove_file(self.make_annotation_path::<T>(&hash, name, version))?;

        // Remove from cache
        self.name_ver_cache
            .get_mut(&get_type_name::<T>())
            .ok_or_else(|| KeyMissingFromBTree {
                key: get_type_name::<T>(),
            })?
            .remove(&Self::make_name_ver_cache_key(name, version));
        Ok(())
    }
}

impl LocalFileStore {
    pub fn new(directory: impl AsRef<Path>) -> Self {
        Self {
            directory: directory.as_ref().into(),
            name_ver_cache: BTreeMap::new(),
        }
    }

    fn make_dir_path<T>(&self, hash: &str) -> PathBuf {
        PathBuf::from(format!(
            "{}/{}/{}",
            self.directory.to_string_lossy(),
            get_type_name::<T>(),
            hash,
        ))
    }

    pub fn make_path<T>(&self, hash: &str, file_name: &str) -> PathBuf {
        self.make_dir_path::<T>(hash).join(file_name)
    }

    pub fn make_annotation_path<T>(&self, hash: &str, name: &str, version: &str) -> PathBuf {
        self.make_dir_path::<T>(hash)
            .join("annotations")
            .join(format!("{name}-{version}.yaml"))
    }

    // Generic function for save load list delete
    /// Generic func to save all sorts of item
    ///
    /// Example usage inside `LocalFileStore`
    /// ``` markdown
    /// let pod = Pod::new(); // For example doesn't actually work
    /// self.save_item(pod, &pod.annotation, &pod.hash).unwrap()
    /// ```
    fn save_item<T: Serialize>(
        &mut self,
        item: &T,
        hash: &str,
        annotation: Option<&Annotation>,
    ) -> Result<()> {
        // Save the item first
        Self::save_file(
            &self.make_path::<T>(hash, "spec.yaml"),
            &to_yaml::<T>(item)?,
            false,
        )?;

        // Save the annotation file and throw and error if exist
        if let Some(value) = annotation {
            // Annotation exist, thus save it
            Self::save_file(
                &self.make_annotation_path::<T>(hash, &value.name, &value.version),
                &serde_yaml::to_string(value)?,
                true,
            )?;

            // Update name-ver_cache
            self.build_cache_if_not_exist::<T>()?;
            self.name_ver_cache
                .get_mut(&get_type_name::<T>())
                .ok_or_else(|| KeyMissingFromBTree {
                    key: get_type_name::<T>(),
                })?
                .insert(format!("{}-{}", value.name, value.version), hash.to_owned());
        }

        Ok(())
    }

    /// Generic function for loading spec.yaml into memory
    fn load_item<T: DeserializeOwned>(&mut self, name: &str, version: &str) -> Result<T> {
        // Search the name-ver index
        let hash = self.get_hash_from_cache::<T>(name, version)?;

        // Get the spec and annotation yaml
        let spec_yaml = fs::read_to_string(self.make_path::<T>(&hash, "spec.yaml"))?;

        let annotation_yaml =
            fs::read_to_string(self.make_annotation_path::<T>(&hash, name, version))?;

        from_yaml::<T>(&spec_yaml, &hash, Some(&annotation_yaml))
    }

    fn delete_item<T>(&mut self, name: &str, version: &str) -> Result<()> {
        // Search the name-ver index
        let hash = self.get_hash_from_cache::<T>(name, version)?;

        // Remove the entire item based on hash
        fs::remove_dir_all(self.make_dir_path::<T>(&hash))?;

        // Delete everyting from cache that has the same hash
        let cache = self
            .name_ver_cache
            .get_mut(&get_type_name::<T>())
            .ok_or_else(|| KeyMissingFromBTree {
                key: get_type_name::<T>(),
            })?;

        // Remove the target key first
        cache.remove(&Self::make_name_ver_cache_key(name, version));

        let mut keys_to_delete: Vec<String> = Vec::new();
        // Search through the remaining values and make sure there is no occurance of hash left
        for (annotation_key, item_hash) in cache.clone() {
            if hash == *item_hash {
                keys_to_delete.push(annotation_key);
            }
        }

        for key in keys_to_delete {
            cache.remove(&key);
        }

        Ok(())
    }

    fn build_name_ver_cache<T>(&mut self) -> Result<()> {
        let type_name = get_type_name::<T>();
        // Check if there is a cache tree for the given object type
        // Construct the cache with glob and regex
        let re = Regex::new(
            r"^.*\/pod\/(?<hash>[a-z0-9]+)\/annotation\/(?<name>[A-z0-9\- ]+)-(?<ver>[0-9]+.[0-9]+.[0-9]+).yaml$",
        )?;

        // Create the missing cache btree for the item
        self.name_ver_cache
            .insert(type_name.clone(), BTreeMap::new());

        let search_pattern = self.make_dir_path::<T>("*").join("annotations/*");

        for path in glob::glob(&search_pattern.to_string_lossy())? {
            let path_str: String = path?.to_string_lossy().to_string();

            let Some(cap) = re.captures(&path_str) else {
                continue;
            };

            // Insert into the cache
            self.name_ver_cache
                .get_mut(&type_name)
                .ok_or_else(|| KeyMissingFromBTree {
                    key: get_type_name::<T>(),
                })?
                .insert(
                    format!("{}-{}", &cap["name"].to_string(), &cap["ver"].to_string()),
                    cap["hash"].into(),
                );
        }
        Ok(())
    }

    fn build_cache_if_not_exist<T>(&mut self) -> Result<()> {
        if !self.name_ver_cache.contains_key(&get_type_name::<T>()) {
            self.build_name_ver_cache::<T>()?;
        }
        Ok(())
    }

    fn get_hash_from_cache<T>(&mut self, name: &str, version: &str) -> Result<String> {
        self.build_cache_if_not_exist::<T>()?;
        let hash = self
            .name_ver_cache
            .get(&get_type_name::<T>())
            .ok_or_else(|| KeyMissingFromBTree {
                key: get_type_name::<T>(),
            })?
            .get(&format!("{name}-{version}"))
            .ok_or_else(|| NoAnnotationFound {
                class: get_type_name::<T>(),
                name: name.into(),
                version: version.into(),
            })?;

        Ok(hash.to_owned())
    }

    fn make_name_ver_cache_key(name: &str, version: &str) -> String {
        format!("{name}-{version}")
    }

    // Help save file function
    fn save_file(path: &PathBuf, content: impl AsRef<[u8]>, fail_if_exists: bool) -> Result<()> {
        fs::create_dir_all(
            path.parent()
                .ok_or_else(|| FileHasNoParent { path: path.clone() })?,
        )?;
        let file_exists = fs::exists(path)?;
        if file_exists {
            if fail_if_exists {
                return Err(FileExists { path: path.clone() }.into());
            }

            println!(
                "Skip saving `{}` since it is already stored.",
                path.to_string_lossy().bright_cyan(),
            );
            return Ok(());
        }

        fs::write(path, content.as_ref())?;
        Ok(())
    }
}
