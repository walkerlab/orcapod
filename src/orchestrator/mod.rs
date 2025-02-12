use crate::{
    error::Result,
    model::{PodJob, PodResult},
    util::get_type_name,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, future::Future, path::PathBuf};
/// Options for sourcing compute environment images.
pub enum ImageKind {
    /// A published compute environment image in a container registry. Argument formatted as
    /// `{server.com/}{name}:{tag}`. Server is optional e.g. (`alpine:latest`).
    Published(String),
    /// A packaged compute environment of image+tag as a tarball. Argument is the relative path of
    /// tarball in orchestrator data directory e.g. (`path/to/image.tar.gz`).
    Tarball(PathBuf),
}
/// Status of a particular compute run.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub enum Status {
    /// Run is ongoing.
    Running,
    /// Run has completed successfully.
    Completed,
    /// Run failed with the provided error code.
    Failed(i16),
}
/// Run metadata
#[derive(Debug)]
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
#[derive(Debug)]
pub struct PodRun {
    /// Original compute request.
    pub pod_job: PodJob,
    /// Name of orchestrator that created the run.
    pub orchestrator_source: String,
    /// Name given by orchestrator.
    pub assigned_name: String,
}

impl PodRun {
    fn new<O: Orchestrator>(pod_job: &PodJob, assigned_name: String) -> Self {
        Self {
            pod_job: pod_job.clone(),
            orchestrator_source: get_type_name::<O>(),
            assigned_name,
        }
    }
}

/// API for standard behavior of any container orchestration engine supported.
pub trait Orchestrator {
    /// How to start containers with an alternate image.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue starting the container.
    fn start_with_altimage(&self, pod_job: &PodJob, image: &ImageKind) -> Result<PodRun>;
    /// How to start containers. Assumes `PodJob` image is published.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue starting the container.
    fn start(&self, pod_job: &PodJob) -> Result<PodRun>;
    /// How to query containers.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue querying metadata from containers.
    fn list(&self) -> Result<Vec<PodRun>>;
    /// How to delete containers.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue deleting a container.
    fn delete(&self, pod_run: &PodRun) -> Result<()>;
    /// How to get container info if still in orchestrator memory.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue accessing container info.
    fn get_info(&self, pod_run: &PodRun) -> Result<RunInfo>;
    /// How to wait for pod result to be ready.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue creating a pod result.
    fn get_result(&self, pod_run: &PodRun) -> Result<PodResult>;
    /// How to (asynchronously) wait for pod result to be ready.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue creating a pod result.
    fn get_result_async(&self, pod_run: &PodRun) -> impl Future<Output = Result<PodResult>> + Send;
}

/// Orchestration implementation for Docker backend.
pub mod docker;
