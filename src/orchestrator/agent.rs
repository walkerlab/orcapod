use crate::{
    error::{OrcaError, Result, selector},
    model::pod::{PodJob, PodStatus},
    orchestrator::{Orchestrator, docker::LocalDockerOrchestrator},
    store::{Store as _, filestore::LocalFileStore},
};
use chrono::{DateTime, Utc};
use colored::Colorize as _;
use derive_more::Display;
use futures_executor::block_on;
use futures_util::future::{FutureExt as _, join_all};
use getset::CloneGetters;
use itertools::Itertools as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use snafu::{OptionExt as _, ResultExt as _};
use std::{
    borrow::ToOwned,
    collections::{BTreeMap, HashMap},
    fmt::Write as _,
    hash::RandomState,
    path::PathBuf,
    sync::Arc,
};
use tokio::{
    sync::mpsc::{self, error::SendError},
    task::JoinSet,
};
use tokio_util::task::TaskTracker;
use uniffi;
use zenoh;

/// A response, similar to Rust's `Result` but casting error to `String`.
///
/// This is a workaround due to `UniFFI` limitations when trying to send a collection of `Result`
/// over the CFFI boundary e.g. concurrent calls where it is necessary to know the status of each
/// request to determine what to retry.
#[derive(uniffi::Enum)]
pub enum Response {
    /// Success
    Ok,
    /// Error cast to `String`
    Err(String),
}

/// Client to connect to an execution agent within a coordinated fleet. Connection optimized/rerouted by Zenoh.
#[expect(
    clippy::field_scoped_visibility_modifiers,
    reason = "Simpler to manage internal access to `session` field via visibility than with custom getter."
)]
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
    pub(crate) session: Arc<zenoh::Session>,
}

#[uniffi::export]
impl AgentClient {
    /// Create a client to connect to the agent network.
    ///
    /// # Errors
    ///
    /// Will fail if there is an issue initializing a session.
    #[uniffi::constructor]
    pub fn new(group: String, host: String) -> Result<Self> {
        Ok(Self {
            group,
            host,
            session: block_on(async {
                Ok::<_, OrcaError>(
                    zenoh::open(zenoh::Config::default())
                        .await
                        .context(selector::AgentCommunicationFailure {})?,
                )
            })?
            .into(),
        })
    }
    /// Start many pod jobs to be processed in parallel.
    /// Return order will match inputs, casting outputs to `String` (since `uniffi` doesn't support sending unwrapped `Result`s).
    pub async fn start_pod_jobs(&self, pod_jobs: Vec<Arc<PodJob>>) -> Vec<Response> {
        join_all(pod_jobs.iter().map(|pod_job| async {
            match self
                .publish(
                    "pod_job",
                    BTreeMap::from([
                        ("action", "request".to_owned()),
                        ("hash", pod_job.hash.clone()),
                    ]),
                    pod_job,
                )
                .await
            {
                Ok(()) => Response::Ok,
                Err(error) => Response::Err(error.to_string()),
            }
        }))
        .await
    }
    /// Watch orchestration agent communication.
    ///
    /// # Errors
    ///
    /// Will fail immediately if there is an error while listening for messages.
    pub async fn watch(&self, key_expr: String) -> Result<()> {
        println!("Watching...");
        let subscriber = self
            .session
            .declare_subscriber(&key_expr)
            .await
            .context(selector::AgentCommunicationFailure {})?;
        loop {
            let sample = subscriber
                .recv_async()
                .await
                .context(selector::AgentCommunicationFailure {})?;
            let value = serde_json::from_slice::<Value>(&sample.payload().to_bytes())?;
            println!("{}: {value:#}", sample.key_expr().as_str().yellow());
        }
    }
}

/// An execution agent.
#[derive(uniffi::Object, CloneGetters, Display, Debug, Clone)]
#[getset(get_clone, impl_attrs = "#[uniffi::export]")]
#[display("{self:#?}")]
#[uniffi::export(Display)]
pub struct Agent {
    /// Connection client for communicating to agent network via self.
    pub client: Arc<AgentClient>,
    /// Associated orchestrator.
    pub orchestrator: Arc<dyn Orchestrator>,
}

#[uniffi::export(async_runtime = "tokio")]
impl Agent {
    /// Create an agent to serve requests for orchestrator processing.
    ///
    /// # Errors
    ///
    /// Will fail if there is an issue initializing its client.
    #[uniffi::constructor]
    pub fn new(
        group: String,
        host: String,
        // todo: UniFFI issue preventing this from working using `Arc<dyn Orchestrator>`` in agent_test.py
        orchestrator: Arc<LocalDockerOrchestrator>,
    ) -> Result<Self> {
        Ok(Self {
            client: AgentClient::new(group, host)?.into(),
            orchestrator,
        })
    }
    /// Start orchestrator execution agent service.
    ///
    /// # Errors
    ///
    /// Will stop and return an error if encounters an error while processing any pod job request.
    pub async fn start(
        &self,
        namespace_lookup: &HashMap<String, PathBuf>,
        available_store: Option<Arc<LocalFileStore>>,
    ) -> Result<()> {
        let mut services = JoinSet::new();
        let self_ref = Arc::new(self.clone());
        services.spawn(start_service(
            Arc::clone(&self_ref),
            "pod_job",
            BTreeMap::from([("action", "request".to_owned())]),
            namespace_lookup.clone(),
            async |agent, inner_namespace_lookup, _, pod_job| {
                let pod_run = agent
                    .orchestrator
                    .start(&pod_job, &inner_namespace_lookup)
                    .await?;
                let pod_result = agent
                    .orchestrator
                    .get_result(&pod_run, &inner_namespace_lookup)
                    .await?;
                agent.orchestrator.delete(&pod_run).await?;
                Ok(pod_result)
            },
            async |client, pod_result| {
                client
                    .publish(
                        "pod_job",
                        BTreeMap::from([
                            (
                                "event",
                                match &pod_result.status {
                                    PodStatus::Completed => "success",
                                    PodStatus::Running
                                    | PodStatus::Failed(_)
                                    | PodStatus::Undefined
                                    | PodStatus::Unset => "failure",
                                }
                                .to_owned(),
                            ),
                            ("hash", pod_result.pod_job.hash.clone()),
                        ]),
                        &pod_result,
                    )
                    .await
            },
        ));
        if let Some(store) = available_store {
            services.spawn(start_service(
                Arc::clone(&self_ref),
                "pod_job",
                BTreeMap::from([("event", "*".to_owned())]),
                namespace_lookup.clone(),
                async move |_, _, _, pod_result| {
                    store.save_pod_result(&pod_result)?;
                    Ok(())
                },
                async |_, ()| Ok(()),
            ));
        }
        services.join_next().await.context(selector::MissingInfo {
            details: "no available services".to_owned(),
        })??
    }
}

pub(crate) fn extract_metadata(key_expr: &str) -> HashMap<String, String> {
    key_expr
        .split('/')
        .map(ToOwned::to_owned)
        .tuples()
        .collect()
}

impl AgentClient {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "write! on a `String` cannot fail. https://rust-lang.github.io/rust-clippy/master/index.html#format_collect"
    )]
    pub(crate) fn make_key_expr(
        &self,
        is_subscriber: bool,
        topic: &str,
        mut metadata: BTreeMap<&str, String>,
    ) -> String {
        metadata.insert("group", self.group.clone());
        metadata.insert("topic", topic.to_owned());

        let delimiter = if is_subscriber {
            "**/".to_owned()
        } else {
            metadata.insert("host", self.host.clone());
            metadata.insert("timestamp", Utc::now().to_rfc3339());
            String::new()
        };

        metadata
            .iter()
            .fold(delimiter.clone(), |mut key_expr, (key, value)| {
                let _ = write!(key_expr, "{key}/{value}/{delimiter}");
                key_expr
            })
            .trim_end_matches('/')
            .to_owned()
    }

    pub(crate) async fn publish<T>(
        &self,
        topic: &str,
        metadata: BTreeMap<&str, String>,
        payload: &T,
    ) -> Result<()>
    where
        T: Serialize + Sync + ?Sized,
    {
        Ok(self
            .session
            .put(
                self.make_key_expr(false, topic, metadata),
                &serde_json::to_vec(payload)?,
            )
            .await
            .context(selector::AgentCommunicationFailure {})?)
    }
    /// Send a log message to the agent network.
    ///
    /// # Errors
    ///
    /// Will fail if there is an issue sending the message.
    pub(crate) async fn log(&self, message: &str) -> Result<()> {
        self.publish("log", BTreeMap::new(), message).await
    }
}

#[expect(
    clippy::excessive_nesting,
    clippy::let_underscore_must_use,
    reason = "`result::Result<(), SendError<_>>` is the only uncaptured result since it would mean we can't transmit results over mpsc."
)]
async fn start_service<
    RequestF,  // function to run on requests
    RequestI,  // input to the function for requests
    RequestR,  // output to the function for requests
    ResponseF, // function to run on completing a request i.e. response
    ResponseI, // input to the function for responses
    ResponseR, // output to the function for responses
>(
    agent: Arc<Agent>,
    request_topic: &str,
    request_metadata: BTreeMap<&'static str, String>,
    namespace_lookup: HashMap<String, PathBuf, RandomState>,
    request_task: RequestF,
    response_task: ResponseF,
) -> Result<()>
where
    RequestI: for<'serde> Deserialize<'serde> + Send + 'static,
    RequestF: FnOnce(
            Arc<Agent>,
            HashMap<String, PathBuf>,
            (DateTime<Utc>, HashMap<String, String>),
            RequestI,
        ) -> RequestR
        + Clone
        + Send
        + 'static,
    RequestR: Future<Output = Result<ResponseI>> + Send + 'static,
    ResponseI: Send + 'static,
    ResponseF: Fn(Arc<AgentClient>, ResponseI) -> ResponseR + Send + 'static,
    ResponseR: Future<Output = Result<()>> + Send + 'static,
{
    agent
        .client
        .log(&format!(
            "Started `{request_topic}` service for {request_metadata:?}."
        ))
        .await?;
    let (response_tx, mut response_rx) = mpsc::channel(100);

    let mut services = JoinSet::new();
    services.spawn({
        let inner_agent = Arc::clone(&agent);
        let inner_request_topic = request_topic.to_owned();
        async move {
            let tasks = TaskTracker::new();
            let subscriber = inner_agent
                .client
                .session
                .declare_subscriber(inner_agent.client.make_key_expr(
                    true,
                    &inner_request_topic,
                    request_metadata,
                ))
                .await
                .context(selector::AgentCommunicationFailure {})?;
            loop {
                let sample = subscriber
                    .recv_async()
                    .await
                    .context(selector::AgentCommunicationFailure {})?;
                let input = serde_json::from_slice::<RequestI>(&sample.payload().to_bytes())?;
                let inner_response_tx = response_tx.clone();
                let mut event_metadata = extract_metadata(sample.key_expr().as_str());
                let timestamp =
                    event_metadata
                        .remove("timestamp")
                        .context(selector::MissingInfo {
                            details: "timestamp",
                        })?;
                let event_timestamp =
                    DateTime::<Utc>::from(DateTime::parse_from_rfc3339(&timestamp)?);
                tasks.spawn({
                    let inner_request_task = request_task.clone();
                    let inner_inner_agent = Arc::clone(&inner_agent);
                    let inner_namespace_lookup = namespace_lookup.clone();
                    async move {
                        inner_request_task(
                            inner_inner_agent,
                            inner_namespace_lookup,
                            (event_timestamp, event_metadata),
                            input,
                        )
                        .then(move |response| async move {
                            let _: Result<(), SendError<Result<ResponseI>>> =
                                inner_response_tx.send(response).await;
                            Ok::<_, OrcaError>(())
                        })
                        .await
                    }
                });
            }
        }
    });
    services.spawn(async move {
        loop {
            let response = response_rx.recv().await.context(selector::MissingInfo {
                details: "channel empty or closed",
            })?;
            response_task(Arc::clone(&agent.client), response?).await?;
        }
    });

    services.join_next().await.context(selector::MissingInfo {
        details: "no available services",
    })??
}
