use crate::{
    error::{OrcaError, OrcaResult},
    model::{from_yaml, to_yaml, Pod},
    store::Store,
};
use colored::Colorize;
use regex::Regex;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
/// Support for a storage backend on a local filesystem directory.
#[derive(Debug)]
pub struct LocalFileStore {
    /// A local path to a directory where store will be located.
    pub directory: PathBuf,
}

impl Store for LocalFileStore {
    fn save_pod(&self, pod: &Pod) -> OrcaResult<()> {
        let class = "pod";
        // Save the annotation file and throw and error if exist
        Self::save_file(
            &self.make_annotation_path(
                class,
                &pod.hash,
                &pod.annotation.name,
                &pod.annotation.version,
            ),
            &serde_yaml::to_string(&pod.annotation)?,
            true,
        )?;
        // Save the pod and skip if it already exist, for the case of many annotation to a single pod
        Self::save_file(
            &self.make_spec_path(class, &pod.hash),
            &to_yaml::<Pod>(pod)?,
            false,
        )?;

        Ok(())
    }

    fn load_pod(&self, name: &str, version: &str) -> OrcaResult<Pod> {
        let class = "pod".to_owned();

        let (_, (hash, _)) =
            Self::parse_annotation_path(&self.make_annotation_path("pod", "*", name, version))?
                .next()
                .ok_or_else(|| {
                    OrcaError::NoAnnotationFound(class, name.to_owned(), version.to_owned())
                })??;

        from_yaml::<Pod>(
            &self.make_annotation_path("pod", &hash, name, version),
            &self.make_spec_path("pod", &hash),
            &hash,
        )
    }

    fn list_pod(&self) -> OrcaResult<BTreeMap<String, Vec<String>>> {
        let (names, (hashes, versions)) =
            Self::parse_annotation_path(&self.make_annotation_path("pod", "*", "*", "*"))?
                .collect::<Result<(Vec<_>, (Vec<_>, Vec<_>)), _>>()?;

        Ok(BTreeMap::from([
            (String::from("name"), names),
            (String::from("hash"), hashes),
            (String::from("version"), versions),
        ]))
    }

    fn delete_pod(&self, name: &str, version: &str) -> OrcaResult<()> {
        // assumes propagate = false
        let class = "pod".to_owned();
        let versions = self.get_pod_version_map(name)?;
        let hash = versions.get(version).ok_or_else(|| {
            OrcaError::NoAnnotationFound(class, name.to_owned(), version.to_owned())
        })?;

        let annotation_file = self.make_annotation_path("pod", hash, name, version);
        let annotation_dir = annotation_file
            .parent()
            .ok_or_else(|| OrcaError::FileHasNoParent(annotation_file.clone()))?;
        let spec_file = self.make_spec_path("pod", hash);
        let spec_dir = spec_file
            .parent()
            .ok_or_else(|| OrcaError::FileHasNoParent(spec_file.clone()))?;

        fs::remove_file(&annotation_file)?;
        if !versions
            .iter()
            .any(|(list_version, list_hash)| list_version != version && list_hash == hash)
        {
            fs::remove_dir_all(spec_dir)?;
        }
        if !versions
            .iter()
            .any(|(list_version, _)| list_version != version)
        {
            fs::remove_dir_all(annotation_dir)?;
        }

        Ok(())
    }
}

impl LocalFileStore {
    /// Construct a local file store instance.
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }
    /// Path where annotation file is located.
    pub fn make_annotation_path(
        &self,
        class: &str,
        hash: &str,
        name: &str,
        version: &str,
    ) -> PathBuf {
        PathBuf::from(format!(
            "{}/{}/{}/{}/{}-{}.yaml",
            self.directory.to_string_lossy(),
            "annotation",
            class,
            name,
            hash,
            version,
        ))
    }
    /// Path where specification file is located.
    pub fn make_spec_path(&self, class: &str, hash: &str) -> PathBuf {
        PathBuf::from(format!(
            "{}/{}/{}/{}",
            self.directory.to_string_lossy(),
            class,
            hash,
            "spec.yaml",
        ))
    }

    fn parse_annotation_path(
        path: &Path,
    ) -> OrcaResult<impl Iterator<Item = OrcaResult<(String, (String, String))>>> {
        let paths = glob::glob(&path.to_string_lossy())?.map(|filepath| {
            let re = Regex::new(
                r"(?x)
                ^.*
                \/(?<name>[0-9a-zA-Z\-]+)
                \/
                    (?<hash>[0-9A-F]+)
                    -
                    (?<version>[0-9]+\.[0-9]+\.[0-9]+)
                    \.yaml
                $",
            )?;
            let filepath_string = String::from(filepath?.to_string_lossy());
            let group = re
                .captures(&filepath_string)
                .ok_or_else(|| OrcaError::NoRegexMatch)?;
            Ok((
                group["name"].to_string(),
                (group["hash"].to_string(), group["version"].to_string()),
            ))
        });
        Ok(paths)
    }

    fn get_pod_version_map(&self, name: &str) -> OrcaResult<BTreeMap<String, String>> {
        Self::parse_annotation_path(&self.make_annotation_path("pod", "*", name, "*"))?
            .map(|metadata| -> OrcaResult<(String, String)> {
                let resolved_metadata = metadata?;
                let hash = resolved_metadata.1 .0;
                let version = resolved_metadata.1 .1;
                Ok((version, hash))
            })
            .collect::<Result<BTreeMap<String, String>, _>>()
    }

    fn save_file(file: &Path, content: &str, fail_if_exists: bool) -> OrcaResult<()> {
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent)?;
        }
        let file_exists = file.exists();
        if file_exists && fail_if_exists {
            return Err(OrcaError::FileExists(file.to_path_buf()));
        } else if file_exists {
            println!(
                "Skip saving `{}` since it is already stored.",
                file.to_string_lossy().bright_cyan(),
            );
        } else {
            fs::write(file, content)?;
        }
        Ok(())
    }
}
