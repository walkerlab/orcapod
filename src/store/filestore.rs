use colored::Colorize as _;
use glob::glob;
use heck::ToSnakeCase as _;
use regex::Regex;
use serde::{Serialize, de::DeserializeOwned};
use serde_yaml;
use snafu::OptionExt as _;
use std::{
    backtrace::Backtrace,
    collections::{HashMap, HashSet},
    fmt, fs,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use crate::{
    crypto::hash_buffer,
    error::{Kind, OrcaError, Result, selector},
    model::{
        Annotation, ModelType, ToYaml,
        pipeline::{JOIN_OPERATOR_HASH, Kernel, NodeURI, Pipeline},
        pod::{Pod, PodJob, PodResult},
    },
    operator::MapOperator,
    store::{MODEL_NAMESPACE, ModelID, ModelInfo, Store},
    util::get_type_name,
};

#[expect(clippy::expect_used, reason = "Valid static regex")]
static RE_MODEL_METADATA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
            ^
                (?<store_directory>.*?)/
                    (?<namespace>[a-z_]+)/
                        (?<class>[a-z_]+)/
                            (?<hash>[0-9a-f]+)/
                                (
                                    annotation/
                                        (?<name>[0-9a-zA-Z\-]+)
                                        -
                                        (?<version>[0-9]+\.[0-9]+\.[0-9]+)
                                        \.yaml
                                |
                                    spec\.yaml
                                )
            $
            ",
    )
    .expect("Invalid model metadata regex.")
});

use chrono::Utc;
use derive_more::Display;
use getset::CloneGetters;
use serde::Deserialize;
use uniffi;
/// Support for a storage backend on a local filesystem directory.
#[derive(uniffi::Object, Debug, Display, CloneGetters, Clone)]
#[getset(get_clone, impl_attrs = "#[uniffi::export]")]
#[display("{self:#?}")]
#[uniffi::export(Display)]
pub struct LocalFileStore {
    /// A local path to a directory where store will be located.
    pub directory: PathBuf,
}

#[uniffi::export]
impl Store for LocalFileStore {
    fn save_pod(&self, pod: &Pod) -> Result<()> {
        self.save_model(pod, &pod.hash, pod.annotation.as_ref())?;
        // Deal with saving the recommended_specs
        // Since we are going with a no modify scheme for saving, we will save the latest version as year-month-day-hour-min-second UTC
        Self::save_file(
            self.make_path::<Pod>(
                &pod.hash,
                format!(
                    "recommended_specs/{}",
                    Utc::now().format("%Y-%m-%d-%H-%M-%S-%f")
                ),
            ),
            &pod.recommend_specs.to_yaml()?,
        )
    }

    fn load_pod(&self, model_id: &ModelID) -> Result<Pod> {
        let (mut pod, annotation, hash) = self.load_model::<Pod>(model_id)?;
        pod.annotation = annotation;
        pod.hash = hash;
        // Deal with the recommended_specs by selecting the last saved spec
        // List all files in the dir
        let folder_path = self.make_path::<Pod>(&pod.hash, "recommended_specs");
        let mut recommended_specs = fs::read_dir(&folder_path)?;

        let mut latest_spec_file_name = recommended_specs
            .next()
            .ok_or(OrcaError {
                kind: Kind::EmptyDir {
                    dir: folder_path.clone(),
                    backtrace: Some(Backtrace::capture()),
                },
            })??
            .file_name();

        for entry in recommended_specs {
            let file_name = entry?.file_name();
            if file_name > latest_spec_file_name {
                latest_spec_file_name = file_name;
            }
        }

        // Read the latest_spec and loaded back in
        pod.recommend_specs = serde_yaml::from_str(&fs::read_to_string(
            folder_path.join(latest_spec_file_name),
        )?)?;

        Ok(pod)
    }

    fn list_pod(&self) -> Result<Vec<ModelInfo>> {
        self.list_model::<Pod>()
    }

    fn delete_pod(&self, model_id: &ModelID) -> Result<()> {
        self.delete_model::<Pod>(model_id)
    }

    fn save_pod_job(&self, pod_job: &PodJob) -> Result<()> {
        self.save_pod(&pod_job.pod)?;
        self.save_model(pod_job, &pod_job.hash, pod_job.annotation.as_ref())
    }

    fn load_pod_job(&self, model_id: &ModelID) -> Result<PodJob> {
        let (mut pod_job, annotation, hash) = self.load_model::<PodJob>(model_id)?;
        pod_job.annotation = annotation;
        pod_job.hash = hash;
        pod_job.pod = self
            .load_pod(&ModelID::Hash(pod_job.pod.hash.clone()))?
            .into();
        Ok(pod_job)
    }

    fn list_pod_job(&self) -> Result<Vec<ModelInfo>> {
        self.list_model::<PodJob>()
    }

    fn delete_pod_job(&self, model_id: &ModelID) -> Result<()> {
        self.delete_model::<PodJob>(model_id)
    }

    fn save_pod_result(&self, pod_result: &PodResult) -> Result<()> {
        self.save_pod_job(&pod_result.pod_job)?;
        self.save_model(pod_result, &pod_result.hash, pod_result.annotation.as_ref())
    }

    fn load_pod_result(&self, model_id: &ModelID) -> Result<PodResult> {
        let (mut pod_result, annotation, hash) = self.load_model::<PodResult>(model_id)?;
        pod_result.annotation = annotation;
        pod_result.hash = hash;
        pod_result.pod_job = self
            .load_pod_job(&ModelID::Hash(pod_result.pod_job.hash.clone()))?
            .into();
        Ok(pod_result)
    }

    fn list_pod_result(&self) -> Result<Vec<ModelInfo>> {
        self.list_model::<PodResult>()
    }

    fn delete_pod_result(&self, model_id: &ModelID) -> Result<()> {
        self.delete_model::<PodResult>(model_id)
    }

    fn delete_annotation(&self, model_type: &ModelType, name: &str, version: &str) -> Result<()> {
        let annotation_file_path = match model_type {
            ModelType::Pod => self.make_path::<Pod>(
                &self.lookup_hash::<Pod>(name, version)?,
                Self::make_annotation_relpath(name, version),
            ),
            ModelType::PodJob => self.make_path::<PodJob>(
                &self.lookup_hash::<PodJob>(name, version)?,
                Self::make_annotation_relpath(name, version),
            ),
            ModelType::PodResult => self.make_path::<PodResult>(
                &self.lookup_hash::<PodResult>(name, version)?,
                Self::make_annotation_relpath(name, version),
            ),
        };
        fs::remove_file(&annotation_file_path)?;

        Ok(())
    }

    fn save_map_operator(&self, map_operator: &MapOperator) -> Result<()> {
        self.save_model(map_operator, &map_operator.hash, None)
    }

    fn load_map_operator(&self, hash: &str) -> Result<MapOperator> {
        let (mut map_operator, _, _) =
            self.load_model::<MapOperator>(&ModelID::Hash(hash.to_owned()))?;
        hash.clone_into(&mut map_operator.hash);
        Ok(map_operator)
    }

    fn list_map_operator(&self) -> Result<Vec<String>> {
        self.list_model::<MapOperator>().map(|infos| {
            infos
                .into_iter()
                .map(|info| info.hash)
                .collect::<Vec<String>>()
        })
    }

    fn delete_map_operator(&self, hash: &str) -> Result<()> {
        self.delete_model::<MapOperator>(&ModelID::Hash(hash.to_owned()))
    }

    fn save_pipeline(&self, pipeline: &Pipeline) -> Result<()> {
        // Save all the kernels first
        for kernel in pipeline.get_kernel_lut() {
            match kernel {
                Kernel::Pod { pod } => self.save_pod(pod)?,
                Kernel::JoinOperator => (), // Skip since it's a constant
                Kernel::MapOperator { mapper } => self.save_map_operator(mapper)?,
            }
        }

        // Save the pipeline
        self.save_model(pipeline, &pipeline.hash, pipeline.annotation.as_ref())?;

        // Get label mapping and hash it
        let labels_lut = pipeline.get_label_lut().collect::<HashMap<_, _>>();
        if !labels_lut.is_empty() {
            let labels_lut_yaml = serde_yaml::to_string(&labels_lut)?;
            let labels_lut_hash = hash_buffer(labels_lut_yaml.as_bytes());
            // Get the latest label file name if exists
            if if let Some(latest_label_file_name) =
                self.get_latest_pipeline_labels_file_name(&pipeline.hash)?
            {
                // Check if the hash is the same
                if latest_label_file_name.split('-').next_back().context(
                    selector::FailedToGetLabelHashFromFileName {
                        file_name: latest_label_file_name.clone(),
                    },
                )? == labels_lut_hash
                {
                    false
                } else {
                    // Hash is different, thus we need to save the new file
                    true
                }
            } else {
                // No existing label file, we need to save the new one
                true
            } {
                Self::save_file(
                    self.make_path::<Pipeline>(
                        &pipeline.hash,
                        format!(
                            "labels/{}-{}",
                            Utc::now().timestamp_millis(),
                            labels_lut_hash
                        ),
                    ),
                    &labels_lut_yaml,
                )?;
            }
        }

        Ok(())
    }

    fn load_pipeline(&self, model_id: &ModelID) -> Result<Pipeline> {
        // Load the file into a temp struct
        #[derive(Deserialize)]
        struct PipelineYaml {
            kernel_lut: HashMap<String, HashSet<String>>,
            dot: String,
            input_spec: HashMap<String, Vec<NodeURI>>,
            output_spec: HashMap<String, NodeURI>,
        }

        let (hash, annotation) = self.decode_model_id::<Pipeline>(model_id)?;

        // Load it from the file
        let pipeline_yaml: PipelineYaml = serde_yaml::from_str(&fs::read_to_string(
            self.make_path::<Pipeline>(&hash, Self::SPEC_RELPATH),
        )?)?;

        // Get all the kernels for the pipeline
        let pod_list = self.list_pod()?;
        let map_operator_list = self.list_map_operator()?;
        let kernels = pipeline_yaml.kernel_lut.into_iter().try_fold(
            HashMap::new(),
            |mut kernels, (kernel_hash, node_hashes)| {
                let kernel = if pod_list.iter().any(|pod_info| pod_info.hash == kernel_hash) {
                    Kernel::Pod {
                        pod: self.load_pod(&ModelID::Hash(kernel_hash))?.into(),
                    }
                } else if map_operator_list
                    .iter()
                    .any(|map_hash| map_hash == &kernel_hash)
                {
                    Kernel::MapOperator {
                        mapper: self.load_map_operator(&kernel_hash)?.into(),
                    }
                } else if kernel_hash == *JOIN_OPERATOR_HASH {
                    Kernel::JoinOperator
                } else {
                    return Err(OrcaError {
                        kind: Kind::MissingInfo {
                            details: format!("Unable to find kernel hash {kernel_hash} in store"),
                            backtrace: Some(Backtrace::capture()),
                        },
                    });
                };
                for node_hash in node_hashes {
                    kernels.insert(node_hash, kernel.clone());
                }
                Ok(kernels)
            },
        )?;

        // Create Pipeline from loaded data
        let mut pipeline = Pipeline::new(
            &pipeline_yaml.dot,
            &kernels,
            pipeline_yaml.input_spec,
            pipeline_yaml.output_spec,
            annotation,
        )?;

        // Get the latest labels
        if let Some(latest_labels_file_name) =
            self.get_latest_pipeline_labels_file_name(&pipeline.hash)?
        {
            let labels_lut: HashMap<String, String> =
                serde_yaml::from_str(&fs::read_to_string(self.make_path::<Pipeline>(
                    &pipeline.hash,
                    format!("labels/{latest_labels_file_name}"),
                ))?)?;

            // Update the nodes with the labels
            pipeline.graph.node_indices().for_each(|node_idx| {
                if let Some(label) = labels_lut.get(&pipeline.graph[node_idx].hash) {
                    pipeline.graph[node_idx].label.clone_from(label);
                }
            });
        }
        // Load the labels LUT
        Ok(pipeline)
    }

    fn list_pipeline(&self) -> Result<Vec<ModelInfo>> {
        self.list_model::<Pipeline>()
    }

    fn delete_pipeline(&self, model_id: &ModelID) -> Result<()> {
        self.delete_model::<Pipeline>(model_id)
    }
}

#[uniffi::export]
impl LocalFileStore {
    /// Construct a local file store instance in a specific directory.
    #[uniffi::constructor]
    pub const fn new(directory: PathBuf) -> Self {
        Self { directory }
    }
}

impl LocalFileStore {
    /// Relative path where model specification is stored within the model directory.
    pub const SPEC_RELPATH: &str = "spec.yaml";
    /// Relative path where model annotation is stored within the model directory.
    pub fn make_annotation_relpath(name: &str, version: &str) -> PathBuf {
        PathBuf::from(format!("annotation/{name}-{version}.yaml"))
    }
    /// Build the storage path with the model directory (`hash`) and a file's relative path.
    pub fn make_path<T: fmt::Debug>(&self, hash: &str, relpath: impl AsRef<Path>) -> PathBuf {
        PathBuf::from(format!(
            "{}/{}/{}/{}",
            self.directory.to_string_lossy(),
            MODEL_NAMESPACE,
            get_type_name::<T>().to_snake_case(),
            hash
        ))
        .join(relpath)
    }

    fn find_model_metadata(glob_pattern: &Path) -> Result<impl Iterator<Item = ModelInfo>> {
        let paths = glob(&glob_pattern.to_string_lossy())?.filter_map(move |filepath| {
            let filepath_string = String::from(filepath.ok()?.to_string_lossy());
            let group = RE_MODEL_METADATA.captures(&filepath_string)?;
            Some(ModelInfo {
                name: group.name("name").map(|name| name.as_str().to_owned()),
                version: group
                    .name("version")
                    .map(|version| version.as_str().to_owned()),
                hash: group["hash"].to_string(),
            })
        });
        Ok(paths)
    }
    /// Find hash using name and version.
    ///
    /// # Errors
    ///
    /// Will return error if unable to find.
    pub(crate) fn lookup_hash<T: fmt::Debug>(&self, name: &str, version: &str) -> Result<String> {
        let model_info = Self::find_model_metadata(
            &self.make_path::<T>("*", Self::make_annotation_relpath(name, version)),
        )?
        .next()
        .context(selector::MissingInfo {
            details: format!(
                "annotation where class = {}, name = {name}, version = {version}",
                get_type_name::<T>().to_snake_case()
            ),
        })?;
        Ok(model_info.hash)
    }

    pub(crate) fn save_file(file: impl AsRef<Path>, content: impl AsRef<[u8]>) -> Result<()> {
        if let Some(parent) = file.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(file, content)?;
        Ok(())
    }
    /// How any model is stored.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue storing the model.
    pub(crate) fn save_model<T: Serialize + fmt::Debug + ToYaml>(
        &self,
        model: &T,
        hash: &str,
        annotation: Option<&Annotation>,
    ) -> Result<()> {
        let class = get_type_name::<T>().to_snake_case();
        // Save annotation if defined and doesn't collide globally i.e. model, name, version
        if let Some(provided_annotation) = annotation {
            let relpath = &Self::make_annotation_relpath(
                &provided_annotation.name,
                &provided_annotation.version,
            );
            if let Some((found_hash, found_name, found_version)) =
                Self::find_model_metadata(&self.make_path::<T>("*", relpath))?
                    .next()
                    .and_then(|model_info| {
                        Some((model_info.hash, model_info.name?, model_info.version?))
                    })
            {
                println!(
                    "{}",
                    format!(
                        "Skip saving {} annotation since `{}`, `{}`, `{}` exists.",
                        class.bright_cyan(),
                        found_hash.bright_cyan(),
                        found_name.bright_cyan(),
                        found_version.bright_cyan(),
                    )
                    .yellow(),
                );
            } else {
                Self::save_file(
                    self.make_path::<T>(hash, relpath),
                    serde_yaml::to_string(provided_annotation)?,
                )?;
            }
        }
        // Save model specification and skip if it already exist e.g. on new annotations
        let spec_file = &self.make_path::<T>(hash, Self::SPEC_RELPATH);
        if spec_file.exists() {
            println!(
                "{}",
                format!(
                    "Skip saving {} model since `{}` exists.",
                    class.bright_cyan(),
                    hash.bright_cyan(),
                )
                .yellow(),
            );
        } else {
            Self::save_file(spec_file, model.to_yaml()?)?;
        }
        Ok(())
    }
    /// How to load any stored model into an instance.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue loading the model from the store using `name` and
    /// `version`.
    pub(crate) fn load_model<T: DeserializeOwned + Default + fmt::Debug>(
        &self,
        model_id: &ModelID,
    ) -> Result<(T, Option<Annotation>, String)> {
        let (hash, annotation) = self.decode_model_id::<T>(model_id)?;

        Ok((
            serde_yaml::from_str(&fs::read_to_string(
                self.make_path::<T>(&hash, Self::SPEC_RELPATH),
            )?)?,
            annotation,
            hash,
        ))
    }

    pub(crate) fn decode_model_id<T: fmt::Debug>(
        &self,
        model_id: &ModelID,
    ) -> Result<(String, Option<Annotation>)> {
        match model_id {
            ModelID::Hash(hash) => Ok((hash.to_owned(), None)),
            ModelID::Annotation(name, version) => {
                let hash = self.lookup_hash::<T>(name, version)?;
                let annotation_str = fs::read_to_string(
                    self.make_path::<T>(&hash, Self::make_annotation_relpath(name, version)),
                )?;
                let annotation: Annotation = serde_yaml::from_str(&annotation_str)?;
                Ok((hash, Some(annotation)))
            }
        }
    }
    /// How to query any stored models.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue querying metadata from existing models in the store.
    pub(crate) fn list_model<T: Default + fmt::Debug>(&self) -> Result<Vec<ModelInfo>> {
        Ok(Self::find_model_metadata(&self.make_path::<T>("**", "*"))?.collect())
    }
    /// How to explicitly delete any stored model and all associated annotations (does not propagate).
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue deleting a model from the store using `name` and
    /// `version`.
    pub(crate) fn delete_model<T: Default + fmt::Debug>(&self, model_id: &ModelID) -> Result<()> {
        // assumes propagate = false
        let hash = match model_id {
            ModelID::Hash(hash) => hash,
            ModelID::Annotation(name, version) => &self.lookup_hash::<T>(name, version)?,
        };
        let spec_dir = self.make_path::<T>(hash, "");
        fs::remove_dir_all(spec_dir)?;

        Ok(())
    }

    pub(crate) fn get_latest_pipeline_labels_file_name(
        &self,
        pipeline_hash: &str,
    ) -> Result<Option<String>> {
        let existing_labels_path = self.make_path::<Pipeline>(pipeline_hash, "labels/");
        Ok(if existing_labels_path.exists() {
            let mut label_file_names = fs::read_dir(&existing_labels_path)?
                .map(|entry| Ok::<_, OrcaError>(entry?.file_name()))
                .collect::<Result<Vec<_>, _>>()?;

            // Sort and get the latest one
            label_file_names.sort();

            label_file_names
                .last()
                .map(|os_str| os_str.to_string_lossy().to_string())
        } else {
            None
        })
    }
}
