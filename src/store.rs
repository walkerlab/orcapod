use crate::model::Pod;
use regex::Regex;
use serde_yaml::Value;
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

pub trait OrcaStore {
    fn save_pod(&self, pod: &Pod) -> Result<(), Box<dyn Error>>;
    fn list_pod(&self) -> Result<BTreeMap<String, Vec<String>>, Box<dyn Error>>;
    // fn load_pod(&self, name: &str, version: &str) -> Result<Option<Pod>, Box<dyn Error>>;
    fn delete_pod(&self, name: &str, version: &str) -> Result<(), Box<dyn Error>>;
}

#[derive(Debug)]
pub struct LocalFileStore {
    pub location: PathBuf,
}

impl OrcaStore for LocalFileStore {
    fn save_pod(&self, pod: &Pod) -> Result<(), Box<dyn Error>> {
        let spec_file = PathBuf::from(format!(
            "{}/{}/{}/{}",
            self.location.display().to_string(),
            pod.class,
            pod.hash,
            "spec.yaml",
        ));
        let spec_yaml = serde_yaml::to_string(pod)?;
        match spec_file.parent() {
            Some(parent) => fs::create_dir_all(&parent)?,
            None => {}
        }
        fs::write(&spec_file, &spec_yaml)?;

        let annotation_file = PathBuf::from(format!(
            "{}/{}/{}/{}/{}-{}.yaml",
            self.location.display().to_string(),
            "annotation",
            pod.class,
            pod.annotation.name,
            pod.hash,
            pod.annotation.version,
        ));
        let annotation_yaml = serde_yaml::to_string(&pod.annotation)?;
        match annotation_file.parent() {
            Some(parent) => fs::create_dir_all(&parent)?,
            None => {}
        }
        fs::write(&annotation_file, &annotation_yaml)?;
        Ok(())
        // println!("spec_file: {:?}", spec_file);
        // println!("annotation_file: {:?}", annotation_file);
        // println!("spec_yaml: {}", spec_yaml);
        // println!("annotation_yaml: {}", annotation_yaml);
    }
    fn list_pod(&self) -> Result<BTreeMap<String, Vec<String>>, Box<dyn Error>> {
        let re = Regex::new(
            r"^.*\/(?<name>[0-9a-zA-Z\-]+)\/(?<hash>[0-9a-f]+)-(?<version>[0-9]+\.[0-9]+\.[0-9]+)\.yaml$",
        )?;

        let paths = glob::glob(&format!(
            "{}/annotation/pod/**/*.yaml",
            self.location.display().to_string(),
        ))?;

        let (names, (hashes, versions)): (Vec<String>, (Vec<String>, Vec<String>)) = paths
            .map(|p| {
                let path_string = &p.unwrap().display().to_string(); // todo: fix unwrap call
                let cap = re.captures(path_string).unwrap();
                (
                    cap["name"].to_string(),
                    (cap["hash"].to_string(), cap["version"].to_string()),
                )
            })
            .unzip();

        Ok(BTreeMap::from([
            (String::from("name"), names),
            (String::from("hash"), hashes),
            (String::from("version"), versions),
        ]))
    }
    // fn load_pod(&self, name: &str, version: &str) -> Result<Option<Pod>, Box<dyn Error>> {}
    fn delete_pod(&self, name: &str, version: &str) -> Result<(), Box<dyn Error>> {
        // todo: need proper logic to remove empty annotation/hash dirs
        let re = Regex::new(
            r"^.*\/(?<name>[0-9a-zA-Z\-]+)\/(?<hash>[0-9a-f]+)-(?<version>[0-9]+\.[0-9]+\.[0-9]+)\.yaml$",
        )?;

        let hash = match glob::glob(&format!(
            "{}/annotation/pod/{}/*-{}.yaml",
            self.location.display().to_string(),
            name,
            version,
        ))?
        .next()
        {
            Some(p) => {
                let path_string = &p?.display().to_string();
                let cap = re.captures(path_string).unwrap();
                Some(cap["hash"].to_string())
            }
            None => None,
        };

        match hash {
            Some(h) => fs::remove_file(&format!(
                "{}/annotation/pod/{}/{}-{}.yaml",
                self.location.display().to_string(),
                h,
                name,
                version,
            ))?,
            None => {}
        }

        Ok(())
    }
}
