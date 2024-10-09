use crate::model::Pod;
use serde_yaml::Value;
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

pub trait OrcaStore {
    fn save_pod(&self, pod: &Pod) -> Result<(), Box<dyn Error>>;
    fn list_pod(&self) -> Result<BTreeMap<String, Vec<String>>, Box<dyn Error>>;
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
        // let file_iter = glob::glob(&format!(
        //     "{}/annotation/pod/**/*.yaml",
        //     self.location.to_str().unwrap()
        // ));
        // let names = file_iter
        //     .unwrap()
        //     .map(|x| x.unwrap().display())
        //     .collect::<Vec<_>>();

        // let names = glob::glob(&format!(
        //     "{}/annotation/pod/**/*.yaml",
        //     self.location.display().to_string(),
        // ))?
        // .collect::<Vec<_>>();

        let names = glob::glob(&format!(
            "{}/annotation/pod/**/*.yaml",
            self.location.display().to_string(),
        ))?
        .map(|x| Ok(x?.display().to_string()))
        .collect::<Result<Vec<String>, Box<dyn Error>>>()?;
        println!("list: {:?}", names);

        Ok(BTreeMap::from([(
            String::from("name"),
            vec![
                String::from("one"),
                String::from("two"),
                String::from("three"),
            ],
        )]))
    }
}
