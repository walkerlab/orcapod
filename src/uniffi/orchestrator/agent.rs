use crate::uniffi::{
    error::{OrcaError, Result, selector},
    model::{PodJob, PodResult},
    orchestrator::{Orchestrator, docker::LocalDockerOrchestrator},
    store::{ModelID, Store},
};
use chrono::{DateTime, Utc};
use derive_more::Display;
use futures_util::future::try_join_all;
use getset::CloneGetters;
use regex::Regex;
use snafu::ResultExt as _;
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex},
};
use tokio::runtime::Runtime;
use uniffi;
use zenoh::{self, bytes::ZBytes};

#[expect(clippy::expect_used, reason = "Valid static regex")]
static RE_PODJOB_ACTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
            ^
                group\/(?<group>[a-z_]+)\/
                    pod_job\/(?<pod_job_hash>[0-9a-f]+)\/
                        (?<action>request|reservation|success|failure)\/
                            host\/(?<host>[a-z_]+)\/
                                timestamp\/(?<timestamp>.*?)
            $
            ",
    )
    .expect("Invalid PodJob action regex.")
});

#[derive(Debug, Clone)]
enum EventPayload {
    Request(PodJob),
    Reservation(ModelID),
    Success(PodResult),
    Failure(PodResult),
}

#[derive(Debug, Clone)]
pub(crate) struct Event {
    group: String,
    host: String,
    subgroup: String,
    payload: EventPayload,
}

/// Agent connection info.
#[derive(uniffi::Object, CloneGetters, Display, Debug, Clone)]
#[getset(get_clone, impl_attrs = "#[uniffi::export]")]
#[display("{self:#?}")]
#[uniffi::export(Display)]
pub struct AgentClient {
    /// Fleet group that is used as a namespace for communication.
    pub group: String,
    /// Connecting agent's assigned name used for reference.
    pub host: String,
    #[getset(skip)]
    session: zenoh::Session,
}

#[uniffi::export]
impl AgentClient {
    /// # Errors
    #[uniffi::constructor]
    pub fn new(group: String, host: String) -> Result<Self> {
        Ok(Self {
            group,
            host,
            session: Runtime::new()?.block_on(async {
                Ok::<zenoh::Session, OrcaError>(
                    zenoh::open(zenoh::Config::default())
                        .await
                        .context(selector::AgentFailure {})?,
                )
            })?,
        })
    }
    /// # Errors
    pub async fn submit_pod_jobs(&self, pod_jobs: Vec<Arc<PodJob>>) -> Result<()> {
        try_join_all(pod_jobs.iter().map(|pod_job| async {
            self.publish(
                &format!("pod_job/{}/request", pod_job.hash),
                &serde_json::to_vec(pod_job)?,
            )
            .await
        }))
        .await?;
        Ok(())
    }
    // /// # Errors
    // pub fn submit_pipeline_job(&self, pipeline_job: &PipelineJob) -> Result<()> {
    //     todo!()
    // }
}

#[uniffi::export]
impl AgentClient {
    /// # Errors
    pub async fn watch_topic(&self) -> Result<()> {
        println!("Watching topics...");
        let subscriber = self
            .session
            .declare_subscriber(format!("group/{}/**", self.group))
            .await
            .context(selector::AgentFailure {})?;
        while let Ok(sample) = subscriber.recv_async().await {
            // println!("Received: {sample:?}");
            println!(
                "{}: {}",
                sample.key_expr().as_str(),
                serde_json::from_slice::<PodJob>(&sample.payload().to_bytes()).map_or_else(
                    |_| sample
                        .payload()
                        .try_to_string()
                        .unwrap_or_else(|error| error.to_string().into())
                        .to_string(),
                    |pod_job| pod_job.to_string()
                )
            );
        }
        Ok(())
    }
}

impl AgentClient {
    async fn publish<'payload, T>(&self, topic: &str, payload: &'payload T) -> Result<()>
    where
        ZBytes: From<&'payload T>,
        T: Sync + ?Sized,
    {
        Ok(self
            .session
            .put(
                format!(
                    "group/{}/{}/host/{}/timestamp/{}",
                    self.group,
                    topic,
                    self.host,
                    Utc::now().to_rfc3339()
                ),
                payload,
            )
            .await
            .context(selector::AgentFailure {})?)
    }
    async fn log(&self, message: &str) -> Result<()> {
        self.publish("log", message).await
    }
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
    #[getset(skip)]
    history: Arc<Mutex<BTreeMap<DateTime<Utc>, Event>>>,
}

#[uniffi::export]
impl Agent {
    /// # Errors
    #[uniffi::constructor]
    pub fn new(
        group: String,
        host: String,
        orchestrator: Arc<LocalDockerOrchestrator>,
    ) -> Result<Self> {
        Ok(Self {
            client: AgentClient::new(group, host)?,
            orchestrator,
            history: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }
    /// # Errors
    /// # Panics
    #[expect(clippy::unwrap_used, clippy::excessive_nesting, reason = "debug")]
    pub async fn start(
        &self,
        namespace_lookup: &HashMap<String, PathBuf>,
        queryable: bool,
        store: Option<Arc<dyn Store>>,
    ) -> Result<()> {
        println!("Agent started...");
        let subscriber = self
            .client
            .session
            .declare_subscriber(format!("group/{}/pod_job/*/request/**", self.client.group))
            .await
            .context(selector::AgentFailure {})?;
        while let Ok(sample) = subscriber.recv_async().await {
            if let (Ok(pod_job), Some(hit)) = (
                serde_json::from_slice::<PodJob>(&sample.payload().to_bytes()),
                RE_PODJOB_ACTION.captures(sample.key_expr().as_str()),
            ) {
                {
                    let mut history = self.history.lock().unwrap();
                    history.insert(
                        DateTime::parse_from_rfc3339(&hit["timestamp"])?.into(),
                        Event {
                            group: hit["group"].to_string(),
                            host: hit["host"].to_string(),
                            subgroup: hit["pod_job_hash"].to_string(),
                            payload: EventPayload::Request(pod_job),
                        },
                    )
                };
                self.client
                    .log(&format!("History: {:#?}", self.history))
                    .await?;
            }
        }
        Ok(())
    }
}
