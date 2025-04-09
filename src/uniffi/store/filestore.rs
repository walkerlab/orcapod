use crate::uniffi::{
    error::Result,
    model::{Pod, PodJob, PodResult},
    store::{ModelID, ModelInfo, Store},
};
use std::{
    fs,
    path::{Path, PathBuf},
};
/// Support for a storage backend on a local filesystem directory.
#[derive(Debug)]
pub struct LocalFileStore {
    /// A local path to a directory where store will be located.
    pub directory: PathBuf,
}

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
        pod_job.pod = self.load_pod(&ModelID::Hash(pod_job.pod.hash))?;
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
        pod_result.pod_job = self.load_pod_job(&ModelID::Hash(pod_result.pod_job.hash))?;
        Ok(pod_result)
    }
    fn list_pod_result(&self) -> Result<Vec<ModelInfo>> {
        self.list_model::<PodResult>()
    }
    fn delete_pod_result(&self, model_id: &ModelID) -> Result<()> {
        self.delete_model::<PodResult>(model_id)
    }
    fn delete_annotation<T>(&self, name: &str, version: &str) -> Result<()> {
        let hash = self.lookup_hash::<T>(name, version)?;
        let annotation_file =
            self.make_path::<T>(&hash, Self::make_annotation_relpath(name, version));
        fs::remove_file(&annotation_file)?;

        Ok(())
    }
}

impl LocalFileStore {
    /// Construct a local file store instance in a specific directory.
    pub fn new(directory: impl AsRef<Path>) -> Self {
        Self {
            directory: directory.as_ref().into(),
        }
    }
}
