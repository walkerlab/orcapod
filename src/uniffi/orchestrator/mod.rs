use crate::uniffi::{
    error::Result,
    model::{OrcaPath, PodJob, PodResult},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use uniffi;
/// Options for sourcing compute environment images.
#[derive(uniffi::Enum)]
pub enum ImageKind {
    /// A published compute environment image in a container registry. Argument formatted as
    /// `{server.com/}{name}:{tag}`. Server is optional e.g. (`alpine:latest`).
    Published(String),
    /// A packaged compute environment of image+tag as a tarball.
    Tarball(OrcaPath),
}
/// Status of a particular compute run.
#[derive(uniffi::Enum, Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Default)]
pub enum Status {
    /// Container is created and is pending execution
    Queued,
    /// Run is ongoing.
    Running,
    /// Run has completed successfully.
    Completed,
    /// Run failed with the provided error code.
    Failed(i16),
    /// Catch all for all undefine behavior
    Unknown,
    /// No status set.
    #[default]
    Unset,
}
/// Run metadata
#[derive(uniffi::Record, Debug)]
pub struct RunInfo {
    /// Environment utilized.
    pub image: String,
    /// Time in epoch when created in seconds.
    pub created: u64,
    /// Time in epoch when terminated in seconds.
    pub terminated: Option<u64>,
    /// Environment variables set in environment.
    pub env_vars: HashMap<String, String>,
    /// Command used to start run.
    pub command: String,
    /// Current run status.
    pub status: Status,
    /// Mounted volume binds to the environment.
    pub mounts: Vec<String>,
    /// Label metadata set by orchestrator.
    pub labels: HashMap<String, String>,
    /// Assigned CPU core limit in fractional cores for the computation.
    pub cpu_limit: f32,
    /// Assigned memory limit in bytes for the computation.
    pub memory_limit: u64,
}
/// Current computation managed by orchestrator.
#[derive(uniffi::Record, Debug, PartialEq)]
pub struct PodRun {
    /// Original compute request.
    pub pod_job: Arc<PodJob>,
    /// Name of orchestrator that created the run.
    pub orchestrator_source: String,
    /// Name given by orchestrator.
    pub assigned_name: String,
}

/// API for standard behavior of any container orchestration engine supported.
#[uniffi::export]
#[async_trait::async_trait]
pub trait Orchestrator: Send + Sync {
    /// How to synchronously start containers with an alternate image.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue starting the container.
    fn start_with_altimage_blocking(
        &self,
        namespace_lookup: &HashMap<String, PathBuf>,
        pod_job: &PodJob,
        image: &ImageKind,
    ) -> Result<PodRun>;
    /// How to synchronously start containers. Assumes `PodJob` image is published.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue starting the container.
    fn start_blocking(
        &self,
        namespace_lookup: &HashMap<String, PathBuf>,
        pod_job: &PodJob,
    ) -> Result<PodRun>;
    /// How to synchronously query containers.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue querying metadata from containers.
    fn list_blocking(&self) -> Result<Vec<PodRun>>;
    /// How to synchronously delete containers.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue deleting a container.
    fn delete_blocking(&self, pod_run: &PodRun) -> Result<()>;
    /// How to synchronously get container info if still in orchestrator memory.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue accessing container info.
    fn get_info_blocking(&self, pod_run: &PodRun) -> Result<RunInfo>;
    /// How to synchronously wait for pod result to be ready.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue creating a pod result.
    fn get_result_blocking(&self, pod_run: &PodRun) -> Result<PodResult>;
    /// How to asynchronously start containers with an alternate image.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue starting the container.
    async fn start_with_altimage(
        &self,
        namespace_lookup: &HashMap<String, PathBuf>,
        pod_job: &PodJob,
        image: &ImageKind,
    ) -> Result<PodRun>;
    /// How to asynchronously start containers. Assumes `PodJob` image is published.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue starting the container.
    async fn start(
        &self,
        namespace_lookup: &HashMap<String, PathBuf>,
        pod_job: &PodJob,
    ) -> Result<PodRun>;
    /// How to asynchronously query containers.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue querying metadata from containers.
    async fn list(&self) -> Result<Vec<PodRun>>;
    /// How to asynchronously delete containers.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue deleting a container.
    async fn delete(&self, pod_run: &PodRun) -> Result<()>;
    /// How to asynchronously get container info if still in orchestrator memory.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue accessing container info.
    async fn get_info(&self, pod_run: &PodRun) -> Result<RunInfo>;
    /// How to asynchronously wait for pod result to be ready.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue creating a pod result.
    async fn get_result(&self, pod_run: &PodRun) -> Result<PodResult>;
}

/// Orchestration implementation for Docker backend.
pub mod docker;
