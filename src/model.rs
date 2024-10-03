use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

// --- core model structs (fields should be in lexicographical order)---

#[derive(Debug, Serialize)]
pub struct Pod {
    annotation: Annotation,
    command: String,
    // gpu: Option<GPUSpecRequirement>
    image: String,                               // Sha256 of docker image hash
    input_stream_map: BTreeMap<String, PathBuf>, // change this back to KeyInfo later
    // might default to /output but needs
    output_dir: PathBuf,
    // here PathBuf will be relative to output_dir
    output_stream_map: BTreeMap<String, PathBuf>,
    recommended_cpus: f32,
    recommended_memory: u64,
    sha256: String,
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
    ) -> Self {
        let resolved_image = String::from(
            "zenmldocker/zenml-server@sha256:78efb7728aac9e4e79966bc13703e7cb239ba9c0eb6322c252bea0399ff2421f",
        );
        let hash = String::from("2e99758548972a8e8822ad47fa1017ff72f06f3ff6a016851f45c398732bc50c");
        Self {
            annotation,
            command,
            image: resolved_image,
            input_stream_map,
            output_dir,
            output_stream_map,
            recommended_cpus,
            recommended_memory,
            sha256: hash,
            source,
        }
    }
}

// --- util structs ---

#[derive(Debug, Serialize)]
pub struct Annotation {
    pub name: String,
    pub version: String,
    pub description: String,
}
