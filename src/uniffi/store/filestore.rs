use crate::uniffi::{
    error::Result,
    model::{ModelType, Pod, PodJob, PodResult},
    store::{ModelID, ModelInfo, Store},
};
use derive_more::Display;
use getset::CloneGetters;
use std::{fs, path::PathBuf};
use uniffi;
/// Support for a storage backend on a local filesystem directory.
#[derive(uniffi::Object, Debug, Display, CloneGetters)]
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
        self.save_model(pod, &pod.hash, pod.annotation.as_ref())
    }
    fn load_pod(&self, model_id: &ModelID) -> Result<Pod> {
        let (mut pod, annotation, hash) = self.load_model::<Pod>(model_id)?;
        pod.annotation = annotation;
        pod.hash = hash;
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
        // Save the logs into a separate file.
        Self::save_file(
            self.make_path(pod_result, &pod_result.hash, "logs.txt"),
            &pod_result.logs,
        )?;
        self.save_model(pod_result, &pod_result.hash, pod_result.annotation.as_ref())
    }
    fn load_pod_result(&self, model_id: &ModelID) -> Result<PodResult> {
        let (mut pod_result, annotation, hash) = self.load_model::<PodResult>(model_id)?;
        pod_result.annotation = annotation;
        pod_result.hash = hash;
        pod_result.pod_job = self
            .load_pod_job(&ModelID::Hash(pod_result.pod_job.hash.clone()))?
            .into();
        pod_result.logs =
            fs::read_to_string(self.make_path(&pod_result, &pod_result.hash, "logs.txt"))?;
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
}

#[uniffi::export]
impl LocalFileStore {
    /// Construct a local file store instance in a specific directory.
    #[uniffi::constructor]
    pub const fn new(directory: PathBuf) -> Self {
        Self { directory }
    }
}
