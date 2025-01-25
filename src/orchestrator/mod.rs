use crate::{error::Result, model::PodJob, store::ModelID};
use std::collections::HashMap;
/// Available states of a run.
#[derive(Debug, PartialEq, Eq)]
pub enum RunState {
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
    /// Name given by orchestrator.
    pub name: String,
    /// Environment utilized.
    pub image: String,
    /// Time in epoch when created.
    pub created: u64,
    /// Environment variables set in environment.
    pub env_vars: HashMap<String, String>,
    /// Command used to start run.
    pub command: String,
    /// Current run state.
    pub state: RunState,
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
pub struct PodRun<'orch, T> {
    pod_job_model_id: ModelID,
    orchestrator: &'orch T,
}
/// API to access `PodRun`-specific orchestrator functionality.
#[expect(
    clippy::new_ret_no_self,
    reason = "Default new implementation should return a `PodRun`."
)]
pub trait PodRunAPI<T> {
    /// How to create a pod run.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue creating a pod run.
    fn new(pod_job_model_id: ModelID, orchestrator: &T) -> Result<PodRun<T>> {
        Ok(PodRun {
            pod_job_model_id,
            orchestrator,
        })
    }
    /// How to get container info if still in orchestrator memory.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue accessing container info.
    fn get_info(&self) -> Result<Option<RunInfo>>;
}

/// API for standard behavior of any container orchestration engine supported.
pub trait API {
    /// An internal type meant to be associated with `PodRun` struct.
    type PodRun<'orch>;
    /// How to start containers.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue starting container.
    fn start(
        &self,
        pod_job: &PodJob,
        env_vars: Option<HashMap<String, String>>,
    ) -> Result<Self::PodRun<'_>>;
    /// How to query containers.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue querying metadata from containers.
    fn list(&self) -> Result<Vec<Self::PodRun<'_>>>;
    /// How to delete containers.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue deleting a container.
    fn delete(&self, pod_run: &Self::PodRun<'_>) -> Result<()>;
}

/// Orchestration implementation for Docker backend.
pub mod docker;
