use crate::util::hash_buffer;
use serde::Serialize;
use std::collections::BTreeMap;
use std::error::Error;
use std::path::PathBuf;

// --- core model structs (fields should be in lexicographical order)---

#[derive(Debug, Serialize)]
pub struct Pod {
    #[serde(skip_serializing)]
    pub annotation: Annotation,
    pub class: String,
    command: String,
    file_content_checksums: BTreeMap<PathBuf, String>,
    // gpu: Option<GPUSpecRequirement>
    #[serde(skip_serializing)]
    pub hash: String,
    image: String,                               // Sha256 of docker image hash
    input_stream_map: BTreeMap<String, PathBuf>, // change this back to KeyInfo later
    // might default to /output but needs
    output_dir: PathBuf,
    // here PathBuf will be relative to output_dir
    output_stream_map: BTreeMap<String, PathBuf>,
    recommended_cpus: f32,
    recommended_memory: u64,
    source: String, // Git Commmit

                    // fn save(&self, fs: FileStore) -> Result {
                    // 	fs.save_pod(*self)
                    // download image.tar.gz on save
                    // }
}

impl Pod {
    pub fn new(
        annotation: Annotation,
        command: String,
        image: String,
        input_stream_map: BTreeMap<String, PathBuf>,
        output_dir: PathBuf,
        output_stream_map: BTreeMap<String, PathBuf>,
        recommended_cpus: f32,
        recommended_memory: u64,
        source: String,
    ) -> Result<Self, Box<dyn Error>> {
        let class = std::any::type_name::<Self>()
            .split("::")
            .collect::<Vec<&str>>()[2]
            .to_lowercase();
        let resolved_image = String::from(
            "zenmldocker/zenml-server@sha256:78efb7728aac9e4e79966bc13703e7cb239ba9c0eb6322c252bea0399ff2421f",
        );
        let checksums = BTreeMap::from([(
            PathBuf::from("image.tar.gz"),
            String::from("78efb7728aac9e4e79966bc13703e7cb239ba9c0eb6322c252bea0399ff2421f"),
        )]);
        let pod_no_hash = Self {
            annotation,
            class,
            command,
            file_content_checksums: checksums,
            hash: String::from(""),
            image: resolved_image,
            input_stream_map,
            output_dir,
            output_stream_map,
            recommended_cpus,
            recommended_memory,
            source,
        };
        let yaml = serde_yaml::to_string(&pod_no_hash)?;
        let hash = hash_buffer(&yaml)?;
        Ok(Self {
            hash,
            ..pod_no_hash
        })
    }
}

// --- util structs ---

#[derive(Debug, Serialize)]
pub struct Annotation {
    pub name: String,
    pub version: String,
    pub description: String,
}
