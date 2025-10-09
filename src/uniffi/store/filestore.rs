use crate::{
    core::model::ToYaml as _,
    uniffi::{
        error::{Kind, OrcaError, Result},
        model::{
            ModelType,
            pipeline::{Kernel, Pipeline},
            pod::{Pod, PodJob, PodResult},
        },
        operator::MapOperator,
        store::{ModelID, ModelInfo, Store},
    },
};
use chrono::Utc;
use derive_more::Display;
use getset::CloneGetters;
use std::{backtrace::Backtrace, fs, path::PathBuf};
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
            self.make_path(
                pod,
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
        let folder_path = self.make_path(&pod, &pod.hash, "recommended_specs");
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
        let annotation_file = self.make_path(
            model_type,
            &self.lookup_hash(model_type, name, version)?,
            Self::make_annotation_relpath(name, version),
        );
        fs::remove_file(&annotation_file)?;

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

        // Save the labels

        Ok(())
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
