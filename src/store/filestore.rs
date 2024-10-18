use crate::{
    error::{FileExists, FileHasNoParent, NoAnnotationFound},
    model::{from_yaml, to_yaml, Annotation, Pod},
    util::get_type_name,
};
use colored::Colorize;
use fs4::fs_std::FileExt;
use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    io::{Read, Seek, Write},
    path::PathBuf,
};

use super::Store;

#[derive(Debug)]
pub struct LocalFileStore {
    pub directory: PathBuf,
}

impl Store for LocalFileStore {
    fn save_pod(&self, pod: &Pod) -> Result<(), Box<dyn Error>> {
        self.save_item(pod, &pod.hash, pod.annotation.as_ref())
    }

    fn load_pod(&self, name: &str, version: &str) -> Result<Pod, Box<dyn Error>> {
        self.load_item::<Pod>(name, version)
    }

    /// Return the name version index btree where key is name-version and value is hash of pod
    fn list_pod(&self) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
        self.get_name_ver_index::<Pod>()
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

    fn make_dir_path<T>(&self, hash: &str) -> PathBuf {
        PathBuf::from(format!(
            "{}/{}/{}",
            self.directory.to_string_lossy(),
            get_type_name::<T>(),
            hash,
        ))
    }

    fn make_path<T>(&self, hash: &str, file_name: &str) -> PathBuf {
        let mut path = self.make_dir_path::<T>(hash);
        path.push(file_name);
        path
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
        hash: &str,
        annotation: Option<&Annotation>,
    ) -> Result<(), Box<dyn Error>> {
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
                &self.make_path::<T>(
                    hash,
                    &format!("{}-{}{}", value.name, value.version, ".yaml"),
                ),
                &serde_yaml::to_string(value)?,
                true,
            )?;

            // Update name-ver index tree
            self.insert_item_to_name_ver_index::<T>(&value.name, &value.version, hash)?;

            let mut file = fs::File::open(self.make_name_ver_index_path::<T>())?;
            let mut temp = Vec::new();
            file.read_to_end(&mut temp)?;
        }

        Ok(())
    }

    /// Generic function for loading spec.yaml into memory
    fn load_item<T: DeserializeOwned>(
        &self,
        name: &str,
        version: &str,
    ) -> Result<T, Box<dyn Error>> {
        // Search the name-ver index
        let hash = self.search_name_ver_index::<T>(name, version)?;

        // Get the spec and annotation yaml
        let spec_yaml = fs::read_to_string(self.make_path::<T>(&hash, "spec.yaml"))?;
        let annotation_yaml =
            fs::read_to_string(self.make_path::<T>(&hash, &format!("{}-{}.yaml", name, version,)))?;

        from_yaml::<T>(&spec_yaml, &hash, Some(&annotation_yaml))
    }

    fn delete_item<T>(&self, name: &str, version: &str) -> Result<(), Box<dyn Error>> {
        // Search the name-ver index
        let hash = self.search_name_ver_index::<T>(name, version)?;

        // Remove actual annotation file
        fs::remove_file(self.make_path::<T>(&hash, &format!("{}-{}.yaml", name, version)))?;

        // Remove the annotation from index tree and see if it is safe to also remove the item folder
        // (Nothing else is reference the hash)
        if self.remove_item_from_name_ver_index::<T>(name, version, &hash)? {
            // No match was found, therefore safe to delete the entire folder
            fs::remove_dir_all(self.make_dir_path::<T>(&hash))?;
        }

        Ok(())
    }

    /// Get the name index btree where key is <name>-<ver> and content is hash of the item
    fn get_name_ver_index<T>(&self) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
        // Get shared lock file
        let mut file = fs::File::open(self.make_name_ver_index_path::<T>())?;
        file.lock_shared()?;

        // Get index_tree then unlock
        let index_tree = Self::extract_index_tree(&mut file)?;
        file.unlock()?;

        Ok(index_tree)
    }

    fn insert_item_to_name_ver_index<T>(
        &self,
        name: &str,
        version: &str,
        hash: &str,
    ) -> Result<(), Box<dyn Error>> {
        let index_file_path = self.make_name_ver_index_path::<T>();

        if !index_file_path.exists() {
            // TODO: Do the glob and regex to construct
            // For no just create the file
            fs::write(
                &index_file_path,
                bincode::serialize(&BTreeMap::<String, String>::new())?,
            )?;
        }

        // Open file and lock exclusive lock it
        let mut file = fs::File::options()
            .read(true)
            .write(true)
            .append(false)
            .open(index_file_path)?;
        file.lock_exclusive()?;

        let mut index_tree = Self::extract_index_tree(&mut file)?;

        // Add item
        index_tree.insert(format!("{}-{}", name, version), hash.into());

        // Rewind to the start then, write
        file.rewind()?;
        file.write_all(&bincode::serialize(&index_tree)?)?;

        file.unlock()?;
        Ok(())
    }

    // Remove from index, and return if it is save to delete the item folder
    fn remove_item_from_name_ver_index<T>(
        &self,
        name: &str,
        version: &str,
        hash: &str,
    ) -> Result<bool, Box<dyn Error>> {
        // Open file and lock exclusive lock it
        let mut file = fs::File::options()
            .read(true)
            .write(true)
            .open(self.make_name_ver_index_path::<T>())?;
        file.lock_exclusive()?;

        let mut index_tree = Self::extract_index_tree(&mut file)?;

        // Remove the annotation file and error out if failed to remove from index_tree
        index_tree
            .remove(&format!("{}-{}", name, version))
            .ok_or(NoAnnotationFound {
                class: get_type_name::<T>(),
                name: name.to_string(),
                version: version.to_string(),
            })?;

        // Write it back to the file
        file.rewind()?;
        file.write_all(&bincode::serialize(&index_tree)?)?;

        // Brute force search through index tree to see if there is another annotation pointing to same hash
        // Maybe optimize this with a btreemap of <hash, vec<annotation_key>> later?
        for (_, item_hash) in index_tree {
            if hash == item_hash {
                // Match found, thus exit
                file.unlock()?;
                return Ok(false);
            }
        }

        file.unlock()?;
        Ok(true)
    }

    fn extract_index_tree(file: &mut fs::File) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        let index_tree: BTreeMap<String, String> = bincode::deserialize(&buf)?;
        Ok(index_tree)
    }

    fn make_name_ver_index_path<T>(&self) -> PathBuf {
        PathBuf::from(format!(
            "{}/{}/name-ver.idx",
            self.directory.to_string_lossy(),
            get_type_name::<T>()
        ))
    }

    /// Search the index BTree and return hash for the item or error out
    fn search_name_ver_index<T>(
        &self,
        name: &str,
        version: &str,
    ) -> Result<String, Box<dyn Error>> {
        // Search the name-ver index
        let name_ver_idx = self.get_name_ver_index::<T>()?;

        // Try to get hash from index, if fail throw error
        Ok(name_ver_idx
            .get(&format!("{}-{}", name, version))
            .ok_or(NoAnnotationFound {
                class: get_type_name::<T>(),
                name: name.to_string(),
                version: version.to_string(),
            })?
            .to_owned())
    }

    // Help save file function
    fn save_file(
        path: &PathBuf,
        content: impl AsRef<[u8]>,
        fail_if_exists: bool,
    ) -> Result<(), Box<dyn Error>> {
        fs::create_dir_all(
            path.parent()
                .ok_or(FileHasNoParent { path: path.clone() })?,
        )?;
        let file_exists = fs::exists(path)?;
        if file_exists {
            if !fail_if_exists {
                println!(
                    "Skip saving `{}` since it is already stored.",
                    path.to_string_lossy().bright_cyan(),
                );
                return Ok(());
            } else {
                return Err(Box::new(FileExists { path: path.clone() }));
            }
        } else {
            fs::write(path, content.as_ref())?;
        }
        Ok(())
    }
}
