use crate::error::{FileHasNoParent, NoAnnotationFound, NoSpecFound};
use crate::model::{from_yaml, to_yaml, Pod};
use crate::util::get_struct_name;
use glob::{GlobError, Paths};
use regex::Regex;
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::iter::Map;
use std::path::PathBuf;

pub trait OrcaStore {
    fn save_pod(&self, pod: &Pod) -> Result<(), Box<dyn Error>>;
    fn list_pod(&self) -> Result<BTreeMap<String, Vec<String>>, Box<dyn Error>>;
    fn load_pod(&self, name: &str, version: &str) -> Result<Pod, Box<dyn Error>>;
    fn delete_pod(&self, name: &str, version: &str) -> Result<(), Box<dyn Error>>;
}

#[derive(Debug)]
pub struct LocalFileStore {
    pub location: PathBuf,
}

impl LocalFileStore {
    fn _parse_annotation_path(
        filepath: &str,
    ) -> Result<
        Map<Paths, impl FnMut(Result<PathBuf, GlobError>) -> (String, (String, String))>,
        Box<dyn Error>,
    > {
        let paths = glob::glob(filepath)?.map(|p| {
            let re = Regex::new(
                r"^.*\/(?<name>[0-9a-zA-Z\-]+)\/(?<hash>[0-9A-F]+)-(?<version>[0-9]+\.[0-9]+\.[0-9]+)\.yaml$",
            ).unwrap();  // todo: fix unsafe
            let path_string = &p.unwrap().display().to_string();  // todo: fix unsafe
            let cap = re.captures(path_string).unwrap();  // todo: fix unsafe
            (
                cap["name"].to_string(),
                (cap["hash"].to_string(), cap["version"].to_string()),
            )
        });

        Ok(paths)
    }
    fn _get_pod_version_map(&self, name: &str) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
        Ok(LocalFileStore::_parse_annotation_path(&format!(
            "{}/annotation/pod/{}/*.yaml",
            self.location.display().to_string(),
            name,
        ))?
        .map(|(_, (h, v))| (v, h))
        .collect::<BTreeMap<String, String>>())
    }
}

impl OrcaStore for LocalFileStore {
    fn save_pod(&self, pod: &Pod) -> Result<(), Box<dyn Error>> {
        // todo: validate version doesn't already exist
        let class = get_struct_name::<Pod>();

        let spec_yaml = to_yaml::<Pod>(&pod)?;
        let spec_file = PathBuf::from(format!(
            "{}/{}/{}/{}",
            self.location.display().to_string(),
            class,
            pod.hash,
            "spec.yaml",
        ));
        fs::create_dir_all(&spec_file.parent().ok_or(FileHasNoParent {
            filepath: spec_file.display().to_string(),
        })?)?;
        fs::write(&spec_file, &spec_yaml)?;

        let annotation_yaml = serde_yaml::to_string(&pod.annotation)?;
        let annotation_file = PathBuf::from(format!(
            "{}/{}/{}/{}/{}-{}.yaml",
            self.location.display().to_string(),
            "annotation",
            class,
            pod.annotation.name,
            pod.hash,
            pod.annotation.version,
        ));
        fs::create_dir_all(&annotation_file.parent().ok_or(FileHasNoParent {
            filepath: annotation_file.display().to_string(),
        })?)?;
        fs::write(&annotation_file, &annotation_yaml)?;

        Ok(())
    }
    fn list_pod(&self) -> Result<BTreeMap<String, Vec<String>>, Box<dyn Error>> {
        let (names, (hashes, versions)): (Vec<String>, (Vec<String>, Vec<String>)) =
            LocalFileStore::_parse_annotation_path(&format!(
                "{}/annotation/pod/**/*.yaml",
                self.location.display().to_string(),
            ))?
            .unzip();

        Ok(BTreeMap::from([
            (String::from("name"), names),
            (String::from("hash"), hashes),
            (String::from("version"), versions),
        ]))
    }
    fn load_pod(&self, name: &str, version: &str) -> Result<Pod, Box<dyn Error>> {
        let class = "pod".to_string();

        let (_, (hash, _)) = LocalFileStore::_parse_annotation_path(&format!(
            "{}/annotation/pod/{}/*-{}.yaml",
            self.location.display().to_string(),
            name,
            version,
        ))?
        .next()
        .ok_or(NoAnnotationFound {
            class: class.clone(),
            name: name.to_string(),
            version: version.to_string(),
        })?;
        let annotation_file = format!(
            "{}/annotation/pod/{}/{}-{}.yaml",
            self.location.display().to_string(),
            name,
            hash,
            version,
        );

        let spec_file = glob::glob(&format!(
            "{}/pod/{}/spec.yaml",
            self.location.display().to_string(),
            hash,
        ))?
        .next()
        .ok_or(NoSpecFound {
            class: class.clone(),
            name: name.to_string(),
            version: version.to_string(),
        })??;

        Ok(from_yaml::<Pod>(
            &annotation_file,
            &spec_file.display().to_string(),
            &hash,
        )?)
    }
    fn delete_pod(&self, name: &str, version: &str) -> Result<(), Box<dyn Error>> {
        // assumes propagate = false
        let versions = LocalFileStore::_get_pod_version_map(&self, name)?;
        let annotation_dir = format!(
            "{}/annotation/pod/{}",
            self.location.display().to_string(),
            name,
        );

        fs::remove_file(&format!(
            "{}/{}-{}.yaml",
            annotation_dir, versions[version], version,
        ))?;
        if versions
            .iter()
            .filter(|&(v, h)| v != version && h == &versions[v])
            .collect::<BTreeMap<_, _>>()
            .is_empty()
        {
            fs::remove_dir_all(&format!(
                "{}/pod/{}",
                self.location.display().to_string(),
                versions[version],
            ))?;
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
