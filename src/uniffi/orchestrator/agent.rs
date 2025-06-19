use crate::{
    core::util::ASYNC_RUNTIME,
    uniffi::{
        error::{OrcaError, Result, selector},
        model::{PodJob, PodResult},
        orchestrator::{Orchestrator, docker::LocalDockerOrchestrator},
        store::{ModelID, Store},
    },
};
use chrono::{DateTime, Utc};
use derive_more::Display;
use futures::{FutureExt as _, TryFutureExt as _};
use futures_util::future::try_join_all;
use getset::CloneGetters;
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use snafu::ResultExt as _;
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex},
    thread::sleep,
    time::Duration,
};
use tokio::{spawn, sync::mpsc, task::JoinSet};
use uniffi;
use zenoh::{self, bytes::ZBytes, sample::Sample};

#[expect(clippy::expect_used, reason = "Valid static regex")]
static RE_PODJOB_ACTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
            ^
                group\/(?<group>[a-z_]+)\/
                    (?<action>request|reservation|success|failure)\/
                        pod_job\/(?<pod_job_hash>[0-9a-f]+)\/
                            host\/(?<host>[a-z_]+)\/
                                timestamp\/(?<timestamp>.*?)
            $
            ",
    )
    .expect("Invalid PodJob action regex.")
});

#[derive(Debug, Clone, Deserialize, Serialize)]
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

type History = BTreeMap<DateTime<Utc>, Event>;

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
    pub session: zenoh::Session,
}

#[uniffi::export]
impl AgentClient {
    /// # Errors
    #[uniffi::constructor]
    pub fn new(group: String, host: String) -> Result<Self> {
        Ok(Self {
            group,
            host,
            session: ASYNC_RUNTIME.block_on(async {
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
                &format!("request/pod_job/{}", pod_job.hash),
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
#[derive(uniffi::Object, CloneGetters, Display, Debug, Clone)]
#[getset(get_clone, impl_attrs = "#[uniffi::export]")]
#[display("{self:#?}")]
#[uniffi::export(Display)]
pub struct Agent {
    /// Client to connect to agent.
    pub client: Arc<AgentClient>,
    /// Associated orchestrator.
    pub orchestrator: Arc<dyn Orchestrator>,
    #[getset(skip)]
    pub history: Option<Arc<Mutex<History>>>,
}

#[uniffi::export]
impl Agent {
    /// # Errors
    #[uniffi::constructor]
    pub fn new(
        group: String,
        host: String,
        orchestrator: Arc<LocalDockerOrchestrator>,
        is_queryable: bool,
    ) -> Result<Self> {
        Ok(Self {
            client: AgentClient::new(group, host)?.into(),
            orchestrator,
            history: is_queryable.then(|| Arc::new(Mutex::new(BTreeMap::new()))),
        })
    }
    /// # Errors
    /// # Panics
    #[expect(clippy::unwrap_used, clippy::excessive_nesting, reason = "debug")]
    pub async fn start_old(
        &self,
        namespace_lookup: &HashMap<String, PathBuf>,
        store: &Option<Arc<dyn Store>>,
    ) -> Result<()> {
        println!("Agent started...");

        let process_pod_job_request =
            async |client: Arc<AgentClient>, history_option: Option<Arc<Mutex<History>>>| {
                let subscriber = client
                    .session
                    .declare_subscriber(format!("group/{}/request/pod_job/**", client.group))
                    .await
                    .context(selector::AgentFailure {})?;
                while let Ok(sample) = subscriber.recv_async().await {
                    if let (Ok(pod_job), Some(capture)) = (
                        serde_json::from_slice::<PodJob>(&sample.payload().to_bytes()),
                        RE_PODJOB_ACTION.captures(sample.key_expr().as_str()),
                    ) {
                        if let Some(history) = &history_option {
                            history.lock().unwrap().insert(
                                DateTime::parse_from_rfc3339(&capture["timestamp"])?.into(),
                                Event {
                                    group: capture["group"].to_string(),
                                    host: capture["host"].to_string(),
                                    subgroup: capture["pod_job_hash"].to_string(),
                                    payload: EventPayload::Request(pod_job),
                                },
                            );
                            client.log(&format!("History: {history:#?}")).await?;
                        }
                        println!("Made it 1");
                        let pod_jobs = serde_json::from_slice::<Vec<PodJob>>(
                            &client
                                .session
                                .get(format!("group/{}/request/pod_job/1123", client.group))
                                .await
                                .context(selector::AgentFailure {})?
                                .iter()
                                .next()
                                .unwrap()
                                .into_result()?
                                .payload()
                                .to_bytes(),
                        )?;
                        println!("Made it 3");

                        for pj in pod_jobs {
                            let message = format!("pod_job command: {}", pj.pod.command);
                            println!("{}", &message);
                            client.log(&message).await?;
                        }
                    }
                }
                Ok::<_, OrcaError>(())
            };

        let process_pod_job_request_query =
            async |client: Arc<AgentClient>,
                   history: Arc<Mutex<BTreeMap<DateTime<Utc>, Event>>>| {
                let queryable = client
                    .session
                    .declare_queryable(format!("group/{}/request/pod_job/**", client.group))
                    .await
                    .context(selector::AgentFailure {})?;
                while let Ok(query) = queryable.recv_async().await {
                    println!("Made it 2");
                    {
                        let pod_jobs = history
                            .lock()
                            .unwrap()
                            .iter()
                            .filter_map(|(_, event)| match &event.payload {
                                EventPayload::Request(pod_job) => Some(pod_job.clone()),
                                EventPayload::Reservation(_)
                                | EventPayload::Success(_)
                                | EventPayload::Failure(_) => None,
                            })
                            .collect::<Vec<_>>();
                        println!("Made it 2.5");
                        query
                            .reply(query.selector().key_expr(), serde_json::to_vec(&pod_jobs)?)
                            .await
                            .context(selector::AgentFailure {})?;
                        println!("Made it 2.75");
                    }
                }
                Ok::<_, OrcaError>(())
            };

        let mut tasks = vec![ASYNC_RUNTIME.spawn(process_pod_job_request(
            Arc::clone(&self.client),
            self.history.clone(),
        ))];

        if let Some(history) = &self.history {
            tasks.push(ASYNC_RUNTIME.spawn(process_pod_job_request_query(
                Arc::clone(&self.client),
                Arc::clone(history),
            )));
        }

        try_join_all(tasks).await?;
        Ok(())
    }

    // /// # Errors
    // /// # Panics
    // #[expect(
    //     clippy::excessive_nesting,
    //     clippy::unused_async,
    //     clippy::unwrap_used,
    //     reason = "debug"
    // )]
    // pub async fn start(
    //     &self,
    //     namespace_lookup: &HashMap<String, PathBuf>,
    //     store: &Option<Arc<dyn Store>>,
    // ) -> Result<()> {
    //     println!("Agent started...");

    //     let mut set = JoinSet::new();
    //     set.spawn(start_service(
    //         self.client.clone(),
    //         self.history.clone(),
    //         "".to_string(),
    //         process_pod_job_request,
    //         Some("".to_string()),
    //     ));
    //     set.join_next().await.unwrap()?
    // }
}

#[expect(
    clippy::excessive_nesting,
    clippy::let_underscore_must_use,
    clippy::unwrap_used,
    reason = "debug"
)]
async fn start_service<OrcaTask, OrcaTaskFuture>(
    client: Arc<AgentClient>,
    history_option: Option<Arc<Mutex<History>>>,
    request_topic: String,
    task: OrcaTask,
    response_topic_option: Option<String>,
) -> Result<()>
where
    // OrcaInput: for<'serde> Deserialize<'serde>,
    // OrcaOutput: Serialize + Sync + Send + 'static,
    // ZBytes: for<'payload> From<&'payload Vec<u8>>,
    OrcaTask: Fn(
            Arc<AgentClient>,
            &EventPayload,
            Option<Arc<Mutex<History>>>,
            &Captures,
        ) -> OrcaTaskFuture
        + Send
        + 'static,
    OrcaTaskFuture: Future<Output = Result<EventPayload>> + Send + 'static,
{
    let (response_tx, mut response_rx) = mpsc::channel(10);

    let mut set = JoinSet::new();
    set.spawn({
        let inner_client = Arc::clone(&client);
        async move {
            let subscriber = inner_client
                .session
                .declare_subscriber(format!("group/{}/{}", inner_client.group, request_topic))
                .await
                .context(selector::AgentFailure {})?;
            while let Ok(sample) = subscriber.recv_async().await {
                if let (Ok(input), Some(metadata)) = (
                    serde_json::from_slice::<EventPayload>(&sample.payload().to_bytes()),
                    RE_PODJOB_ACTION.captures(sample.key_expr().as_str()),
                ) {
                    if let Some(history) = &history_option {
                        history.lock().unwrap().insert(
                            DateTime::parse_from_rfc3339(&metadata["timestamp"])?.into(),
                            Event {
                                group: metadata["group"].to_string(),
                                host: metadata["host"].to_string(),
                                subgroup: metadata["pod_job_hash"].to_string(),
                                payload: input.clone(),
                            },
                        );
                        inner_client.log(&format!("History: {history:#?}")).await?;
                    }
                    let inner_response_tx = response_tx.clone();
                    spawn(
                        task(
                            Arc::clone(&inner_client),
                            &input,
                            history_option.clone(),
                            &metadata,
                        )
                        .then(move |response| async move {
                            let _: Result<(), mpsc::error::SendError<Result<EventPayload>>> =
                                inner_response_tx.send(response).await; // result can't be captured anyway
                            Ok::<_, OrcaError>(())
                        }),
                    );
                }
            }
            Ok(())
        }
    });
    set.spawn(async move {
        while let Some(content) = response_rx.recv().await {
            let payload = &content?;
            if let Some(response_topic) = &response_topic_option {
                client
                    .publish(response_topic, &serde_json::to_vec(payload)?)
                    .await?;
            }
        }
        Ok(())
    });

    set.join_next().await.unwrap()?
}

#[expect(
    clippy::unused_async,
    clippy::unwrap_used,
    clippy::panic,
    reason = "debug"
)]
async fn process_pod_job_request(
    client: Arc<AgentClient>,
    input: &EventPayload,
    history_option: Option<Arc<Mutex<History>>>,
    metadata: &Captures<'_>,
) -> Result<EventPayload> {
    // if let Some(history) = &history_option {
    //     history.lock().unwrap().insert(
    //         DateTime::parse_from_rfc3339(&metadata["timestamp"])?.into(),
    //         Event {
    //             group: metadata["group"].to_string(),
    //             host: metadata["host"].to_string(),
    //             subgroup: metadata["pod_job_hash"].to_string(),
    //             payload: EventPayload::Request(input.clone()),
    //         },
    //     );
    //     client.log(&format!("History: {history:#?}")).await?;
    // }

    // println!("Made it 1");
    // let pod_jobs = serde_json::from_slice::<Vec<PodJob>>(
    //     &client
    //         .session
    //         .get(format!("group/{}/request/pod_job/1123", client.group))
    //         .await
    //         .context(selector::AgentFailure {})?
    //         .iter()
    //         .next()
    //         .unwrap()
    //         .into_result()?
    //         .payload()
    //         .to_bytes(),
    // )?;
    // println!("Made it 3");

    // for pj in pod_jobs {
    //     let message = format!("pod_job command: {}", pj.pod.command);
    //     println!("{}", &message);
    //     client.log(&message).await?;
    // }

    match &input {
        EventPayload::Request(pod_job) => {
            let annotation = pod_job.annotation.as_ref().unwrap();
            Ok(EventPayload::Reservation(ModelID::Annotation(
                annotation.name.clone(),
                annotation.version.clone(),
            )))
        }
        EventPayload::Reservation(_) | EventPayload::Success(_) | EventPayload::Failure(_) => {
            panic!("whoops")
        }
    }
}
