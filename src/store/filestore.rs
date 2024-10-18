use crate::{
    error::{FileExists, FileHasNoParent, NoAnnotationFound, NoRegexMatch},
    model::{from_yaml, to_yaml, Pod},
    store::Store,
};
use alloc::collections::BTreeMap;
use colored::Colorize;
use core::{error::Error, iter::Map};
use glob::{GlobError, Paths};
use regex::Regex;
use std::{
    fs,
    path::{Path, PathBuf},
};

extern crate alloc;

#[derive(Debug)]
pub struct LocalFileStore {
    pub directory: PathBuf,
}

impl Store for LocalFileStore {
    fn save_pod(&self, pod: &Pod) -> Result<(), Box<dyn Error>> {
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

    fn load_pod(&self, name: &str, version: &str) -> Result<Pod, Box<dyn Error>> {
        let class = "pod".to_owned();

        let (_, (hash, _)) =
            Self::parse_annotation_path(&self.make_annotation_path("pod", "*", name, version))?
                .next()
                .ok_or_else(|| NoAnnotationFound {
                    class,
                    name: name.to_owned(),
                    version: version.to_owned(),
                })??;

        from_yaml::<Pod>(
            &self.make_annotation_path("pod", &hash, name, version),
            &self.make_spec_path("pod", &hash),
            &hash,
        )
    }

    fn list_pod(&self) -> Result<BTreeMap<String, Vec<String>>, Box<dyn Error>> {
        let (names, (hashes, versions)) =
            Self::parse_annotation_path(&self.make_annotation_path("pod", "*", "*", "*"))?
                .collect::<Result<(Vec<_>, (Vec<_>, Vec<_>)), _>>()?;

        Ok(BTreeMap::from([
            (String::from("name"), names),
            (String::from("hash"), hashes),
            (String::from("version"), versions),
        ]))
    }

    fn delete_pod(&self, name: &str, version: &str) -> Result<(), Box<dyn Error>> {
        // assumes propagate = false
        let class = "pod".to_owned();
        let versions = self.get_pod_version_map(name)?;
        let hash = versions.get(version).ok_or_else(|| NoAnnotationFound {
            class,
            name: name.to_owned(),
            version: version.to_owned(),
        })?;

        let annotation_file = self.make_annotation_path("pod", hash, name, version);
        let annotation_dir = annotation_file.parent().ok_or_else(|| FileHasNoParent {
            path: annotation_file.clone(),
        })?;
        let spec_file = self.make_spec_path("pod", hash);
        let spec_dir = spec_file.parent().ok_or_else(|| FileHasNoParent {
            path: spec_file.clone(),
        })?;

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
    pub fn new<T: Into<PathBuf>>(location: T) -> Self {
        Self {
            directory: location.into(),
        }
    }

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
    ) -> Result<
        Map<
            Paths,
            impl FnMut(Result<PathBuf, GlobError>) -> Result<(String, (String, String)), Box<dyn Error>>,
        >,
        Box<dyn Error>,
    > {
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
            let group = re.captures(&filepath_string).ok_or(NoRegexMatch {})?;
            Ok((
                group["name"].to_string(),
                (group["hash"].to_string(), group["version"].to_string()),
            ))
        });

        Ok(paths)
    }

    fn get_pod_version_map(&self, name: &str) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
        Self::parse_annotation_path(&self.make_annotation_path("pod", "*", name, "*"))?
            .map(|metadata| -> Result<(String, String), Box<dyn Error>> {
                let resolved_metadata = metadata?;
                let hash = resolved_metadata.1 .0;
                let version = resolved_metadata.1 .1;
                Ok((version, hash))
            })
            .collect::<Result<BTreeMap<String, String>, _>>()
    }

    fn save_file(file: &Path, content: &str, fail_if_exists: bool) -> Result<(), Box<dyn Error>> {
        fs::create_dir_all(file.parent().ok_or_else(|| FileHasNoParent {
            path: file.to_path_buf(),
        })?)?;
        let file_exists = fs::exists(file)?;
        if file_exists && fail_if_exists {
            return Err(Box::new(FileExists {
                path: file.to_path_buf(),
            }));
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
