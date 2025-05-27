use crate::uniffi::{
    error::{OrcaError, Result, selector},
    model::{OrcaPath, PodJob, PodResult},
};
use chrono::Utc;
use derive_more::Display;
use docker::LocalDockerOrchestrator;
use getset::CloneGetters;
use serde::{Deserialize, Serialize};
use snafu::ResultExt as _;
use std::{collections::HashMap, fmt, path::PathBuf, sync::Arc};
use tokio::runtime::Runtime;
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
    /// Run is ongoing.
    Running,
    /// Run has completed successfully.
    Completed,
    /// Run failed with the provided error code.
    Failed(i16),
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
#[derive(uniffi::Record, Debug)]
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
pub trait Orchestrator: Send + Sync + fmt::Debug {
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
}

/// Agent connection info.
#[derive(uniffi::Object, CloneGetters, Display, Debug, Clone)]
#[getset(get_clone, impl_attrs = "#[uniffi::export]")]
#[display("{self:#?}")]
#[uniffi::export(Display)]
pub struct AgentClient {
    /// Fleet group that is used as a namespace for communication.
    pub group: String,
    /// An assigned name for reference.
    pub name: String,
    #[getset(skip)]
    session: zenoh::Session,
}

#[uniffi::export]
impl AgentClient {
    /// # Errors
    #[uniffi::constructor]
    pub fn new(group: String, name: String) -> Result<Self> {
        Ok(Self {
            group,
            name,
            session: Runtime::new()?.block_on(async {
                Ok::<zenoh::Session, OrcaError>(
                    zenoh::open(zenoh::Config::default())
                        .await
                        .context(selector::AgentFailure {})?,
                )
            })?,
        })
    }
    async fn write_topic_data(&self, rel_topic: &str, data: &PodJob) -> Result<()> {
        self.session
            .put(
                format!("{}{}", self.group, rel_topic),
                serde_json::to_vec(&data)?,
            )
            .await
            .context(selector::AgentFailure {})?;
        Ok(())
    }
    async fn write_topic_str(&self, rel_topic: &str, message: &str) -> Result<()> {
        self.session
            .put(format!("{}{}", self.group, rel_topic), message)
            .await
            .context(selector::AgentFailure {})?;
        Ok(())
    }
    async fn watch_topic(&self) -> Result<()> {
        let subscriber = self
            .session
            .declare_subscriber(format!("{}{}", self.group, "/**"))
            .await
            .context(selector::AgentFailure {})?;
        while let Ok(sample) = subscriber.recv_async().await {
            // println!("Received: {sample:?}");
            if let Ok(data) = serde_json::from_slice::<PodJob>(&sample.payload().to_bytes()) {
                println!("Subscribed: {data:?}");
            } else {
                println!(
                    "[{}][{}]: {}",
                    sample.key_expr().as_str(),
                    Utc::now(),
                    sample
                        .payload()
                        .try_to_string()
                        .unwrap_or_else(|error| error.to_string().into())
                );
            }
        }
        Ok(())
    }
    /// # Errors
    pub async fn log(&self, session_id: &str, message: &str) -> Result<()> {
        self.write_topic_str(
            &format!("/session/{session_id}/log/host/{}", self.name),
            message,
        )
        .await
    }
    // /// # Errors
    // pub fn submit_pod_jobs(&self, pod_jobs: Vec<Arc<PodJob>>) -> Result<()> {
    //     todo!()
    // }
    // pub fn submit_pipeline_job(&self, pipeline_job: &PipelineJob) -> Result<()> {
    //     todo!()
    // }
}

/// An execution agent.
#[derive(uniffi::Object, CloneGetters, Display, Debug)]
#[getset(get_clone, impl_attrs = "#[uniffi::export]")]
#[display("{self:#?}")]
#[uniffi::export(Display)]
pub struct Agent {
    /// Client to connect to agent.
    pub client: AgentClient,
    /// Associated orchestrator.
    pub orchestrator: Arc<dyn Orchestrator>,
}

#[uniffi::export]
impl Agent {
    /// # Errors
    #[uniffi::constructor]
    pub fn new(
        group: String,
        name: String,
        orchestrator: Arc<LocalDockerOrchestrator>,
    ) -> Result<Self> {
        Ok(Self {
            client: Runtime::new()?.block_on(async { AgentClient::new(group, name) })?,
            orchestrator,
        })
    }
    // /// # Errors
    // pub async fn start(
    //     &self,
    //     namespace_lookup: &HashMap<String, PathBuf>,
    //     queryable: bool,
    //     store: Option<Arc<dyn Store>>,
    // ) -> Result<()> {
    //     todo!()
    // }
}

/// Orchestration implementation for Docker backend.
pub mod docker;
