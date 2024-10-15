use crate::{
    error::{FileExists, FileHasNoParent, NoAnnotationFound, NoRegexMatch},
    model::{from_yaml, to_yaml, Pod},
    store::Store,
};
use colored::Colorize;
use glob::{GlobError, Paths};
use regex::Regex;
use std::{collections::BTreeMap, error::Error, fs, iter::Map, path::PathBuf};

#[derive(Debug)]
pub struct LocalFileStore {
    pub directory: PathBuf,
}

impl Store for LocalFileStore {
    fn save_pod(&self, pod: &Pod) -> Result<(), Box<dyn Error>> {
        let class = "pod";

        // Save the annotation file and throw and error if exist
        LocalFileStore::save_file(
            &self.make_annotation_path(
                &class,
                &pod.hash,
                &pod.annotation.name,
                &pod.annotation.version,
            ),
            &serde_yaml::to_string(&pod.annotation)?,
            true,
        )?;

        // Save the pod and skip if it already exist, for the case of many annotation to a single pod
        LocalFileStore::save_file(
            &self.make_spec_path(&class, &pod.hash),
            &to_yaml::<Pod>(&pod)?,
            false,
        )?;

        Ok(())
    }

    fn load_pod(&self, name: &str, version: &str) -> Result<Pod, Box<dyn Error>> {
        let class = "pod".to_string();

        let (_, (hash, _)) = LocalFileStore::parse_annotation_path(
            &self.make_annotation_path("pod", "*", &name, &version),
        )?
        .next()
        .ok_or(NoAnnotationFound {
            class: class.clone(),
            name: name.to_string(),
            version: version.to_string(),
        })??;

        Ok(from_yaml::<Pod>(
            &self.make_annotation_path("pod", &hash, &name, &version),
            &self.make_spec_path("pod", &hash),
            &hash,
        )?)
    }

    fn list_pod(&self) -> Result<BTreeMap<String, Vec<String>>, Box<dyn Error>> {
        let (names, (hashes, versions)) = LocalFileStore::parse_annotation_path(
            &self.make_annotation_path("pod", "*", "*", "*"),
        )?
        .collect::<Result<(Vec<_>, (Vec<_>, Vec<_>)), _>>()?;

        Ok(BTreeMap::from([
            (String::from("name"), names),
            (String::from("hash"), hashes),
            (String::from("version"), versions),
        ]))
    }

    fn delete_pod(&self, name: &str, version: &str) -> Result<(), Box<dyn Error>> {
        // assumes propagate = false
        let versions = LocalFileStore::get_pod_version_map(&self, name)?;
        let annotation_file = self.make_annotation_path("pod", &versions[version], &name, &version);
        let annotation_dir = annotation_file.parent().ok_or(FileHasNoParent {
            path: annotation_file.clone(),
        })?;
        let spec_file = self.make_spec_path("pod", &versions[version]);
        let spec_dir = spec_file.parent().ok_or(FileHasNoParent {
            path: spec_file.clone(),
        })?;

        fs::remove_file(&annotation_file)?;
        if versions
            .iter()
            .filter(|&(v, h)| v != version && h == &versions[v])
            .collect::<BTreeMap<_, _>>()
            .is_empty()
        {
            fs::remove_dir_all(&spec_dir)?;
        }
        if versions
            .iter()
            .filter(|&(v, _)| v != version)
            .collect::<BTreeMap<_, _>>()
            .is_empty()
        {
            fs::remove_dir_all(&annotation_dir)?;
        }

        Ok(())
    }
}

impl LocalFileStore {
    pub fn new(location: impl Into<PathBuf>) -> Self {
        Self {
            directory: location.into(),
        }
    }

    fn make_annotation_path(&self, class: &str, hash: &str, name: &str, version: &str) -> PathBuf {
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

    fn make_spec_path(&self, class: &str, hash: &str) -> PathBuf {
        PathBuf::from(format!(
            "{}/{}/{}/{}",
            self.directory.to_string_lossy(),
            class,
            hash,
            "spec.yaml",
        ))
    }

    fn parse_annotation_path(
        path: &PathBuf,
    ) -> Result<
        Map<
            Paths,
            impl FnMut(Result<PathBuf, GlobError>) -> Result<(String, (String, String)), Box<dyn Error>>,
        >,
        Box<dyn Error>,
    > {
        let paths = glob::glob(&path.to_string_lossy())?.map(|p| {
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
            let path = p?;
            let path_string = &path.to_string_lossy();
            let cap = re.captures(&path_string).ok_or(NoRegexMatch {})?;
            Ok((
                cap["name"].to_string(),
                (cap["hash"].to_string(), cap["version"].to_string()),
            ))
        });

        Ok(paths)
    }

    fn get_pod_version_map(&self, name: &str) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
        Ok(LocalFileStore::parse_annotation_path(
            &self.make_annotation_path("pod", "*", name, "*"),
        )?
        .map(|m| -> Result<(String, String), Box<dyn Error>> {
            let metadata = m?.clone();
            let hash = metadata.1 .0;
            let version = metadata.1 .1;
            Ok((version, hash))
        })
        .collect::<Result<BTreeMap<String, String>, _>>()?)
    }

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
