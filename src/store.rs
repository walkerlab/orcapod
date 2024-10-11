use crate::error::NotFound;
use crate::model::{from_yaml, to_yaml, Pod};
use crate::util::get_struct_name;
use regex::Regex;
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

pub trait OrcaStore {
    const ANNOTATION_FILE_REGEX: &str;
    fn save_pod(&self, pod: &Pod) -> Result<(), Box<dyn Error>>;
    fn list_pod(&self) -> Result<BTreeMap<String, Vec<String>>, Box<dyn Error>>;
    fn load_pod(&self, name: &str, version: &str) -> Result<Pod, Box<dyn Error>>;
    fn delete_pod(&self, name: &str, version: &str) -> Result<(), Box<dyn Error>>;
}

#[derive(Debug)]
pub struct LocalFileStore {
    pub location: PathBuf,
}

impl OrcaStore for LocalFileStore {
    const ANNOTATION_FILE_REGEX: &str = r"^.*\/(?<name>[0-9a-zA-Z\-]+)\/(?<hash>[0-9A-F]+)-(?<version>[0-9]+\.[0-9]+\.[0-9]+)\.yaml$";
    fn save_pod(&self, pod: &Pod) -> Result<(), Box<dyn Error>> {
        let class = get_struct_name::<Pod>();
        let spec_yaml = to_yaml::<Pod>(&pod)?;
        let spec_file = PathBuf::from(format!(
            "{}/{}/{}/{}",
            self.location.display().to_string(),
            class,
            pod.hash,
            "spec.yaml",
        ));

        match spec_file.parent() {
            Some(parent) => fs::create_dir_all(&parent)?,
            None => {}
        }
        fs::write(&spec_file, &spec_yaml)?;

        let annotation_file = PathBuf::from(format!(
            "{}/{}/{}/{}/{}-{}.yaml",
            self.location.display().to_string(),
            "annotation",
            class,
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
        let re = Regex::new(LocalFileStore::ANNOTATION_FILE_REGEX)?;

        let paths = glob::glob(&format!(
            "{}/annotation/pod/**/*.yaml",
            self.location.display().to_string(),
        ))?;

        let (names, (hashes, versions)): (Vec<String>, (Vec<String>, Vec<String>)) = paths
            .map(|p| {
                let path_string = &p.unwrap().display().to_string(); // todo: fix unsafe
                let cap = re.captures(path_string).unwrap(); // todo: fix unsafe
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
    fn load_pod(&self, name: &str, version: &str) -> Result<Pod, Box<dyn Error>> {
        let re = Regex::new(LocalFileStore::ANNOTATION_FILE_REGEX)?;

        let annotation_file = glob::glob(&format!(
            "{}/annotation/pod/{}/*-{}.yaml",
            self.location.display().to_string(),
            name,
            version,
        ))?
        .next()
        .ok_or(NotFound {
            model: "pod",
            name: "something",
            version: "0.1.0",
        })??;

        let hash = re
            .captures(&annotation_file.display().to_string())
            .ok_or(NotFound {
                model: "pod",
                name: "something",
                version: "0.1.0",
            })?["hash"]
            .to_string();

        let spec_file = glob::glob(&format!(
            "{}/pod/{}/spec.yaml",
            self.location.display().to_string(),
            hash,
        ))?
        .next()
        .ok_or(NotFound {
            model: "pod",
            name: "something",
            version: "0.1.0",
        })??;

        Ok(from_yaml::<Pod>(
            &annotation_file.display().to_string(),
            &spec_file.display().to_string(),
            &hash,
        )?)

        // let hash = match glob::glob(&format!(
        //     "{}/annotation/pod/{}/*-{}.yaml",
        //     self.location.display().to_string(),
        //     name,
        //     version,
        // ))?
        // .next()
        // {
        //     Some(p) => {
        //         let path_string = &p?.display().to_string();
        //         let cap = re.captures(path_string).unwrap();
        //         Some(cap["hash"].to_string())
        //     }
        //     None => None,
        // };

        // let annotation_file = match hash {
        //     Some(h) => Some(format!(
        //         "{}/annotation/pod/{}/{}-{}.yaml",
        //         self.location.display().to_string(),
        //         h,
        //         name,
        //         version,
        //     )),
        //     None => None,
        // };
    }
    fn delete_pod(&self, name: &str, version: &str) -> Result<(), Box<dyn Error>> {
        // todo: need proper logic to remove empty annotation/hash dirs
        // in progress...
        let re = Regex::new(LocalFileStore::ANNOTATION_FILE_REGEX)?;

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
                let cap = re.captures(path_string).unwrap(); // todo: fix unsafe
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
