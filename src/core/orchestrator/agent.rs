use crate::uniffi::{
    error::{OrcaError, Result, selector},
    model::pod::{PodJob, PodResult},
    orchestrator::agent::{Agent, AgentClient},
    store::ModelID,
};
use chrono::Utc;
use futures_util::future::FutureExt as _;
use regex::Regex;
use serde::{Deserialize, Serialize};
use snafu::{OptionExt as _, ResultExt as _};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, LazyLock},
};
use tokio::{
    sync::mpsc::{self, error::SendError},
    task::JoinSet,
};
use tokio_util::task::TaskTracker;

#[expect(clippy::expect_used, reason = "Valid static regex")]
static RE_PODJOB_ACTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
            ^
                group\/(?<group>[a-z_\-]+)\/
                    (?<action>request|reservation|success|failure)\/
                        pod_job\/(?<pod_job_hash>[0-9a-f]+)\/
                            host\/(?<host>[0-9a-z_]+)\/
                                timestamp\/(?<timestamp>.*?)
            $
            ",
    )
    .expect("Invalid PodJob action regex.")
});

#[expect(
    dead_code,
    reason = "Need to be able to initialize to pass metadata as input."
)]
#[derive(Debug, Clone)]
pub struct EventMetadata {
    group: String,
    host: String,
    subgroup: String,
}

#[expect(
    dead_code,
    reason = "Need to be able to initialize to pass metadata as input."
)]
#[derive(Debug, Clone)]
pub enum EventPayload {
    Request(PodJob),
    Reservation(ModelID),
    Success(PodResult),
    Failure(PodResult),
}

#[expect(
    dead_code,
    reason = "Need to be able to initialize to pass metadata as input."
)]
#[derive(Debug, Clone)]
pub struct Event {
    metadata: EventMetadata,
    payload: EventPayload,
}

impl AgentClient {
    pub(crate) async fn publish<T>(&self, topic: &str, payload: &T) -> Result<()>
    where
        T: Serialize + Sync + ?Sized,
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
        println!("{message}");
        self.publish("log", message).await
    }
}

#[expect(
    clippy::let_underscore_must_use,
    clippy::excessive_nesting,
    reason = "`result::Result<(), SendError<_>>` is the only uncaptured result since it would mean we can't transmit results over mpsc."
)]
pub async fn start_service<
    EventClassifierF, // function to classify the event payload e.g. EventPayload::{Request | Reservation | ..}
    RequestF,         // function to run on requests
    RequestI,         // input to the function for requests
    RequestR,         // output to the function for requests
    ResponseF,        // function to run on completing a request i.e. response
    ResponseI,        // input to the function for responses
    ResponseR,        // output to the function for responses
>(
    agent: Arc<Agent>,
    request_key_expr: String,
    namespace_lookup: HashMap<String, PathBuf>,
    event_classifier: EventClassifierF,
    request_task: RequestF,
    response_task: ResponseF,
) -> Result<()>
where
    EventClassifierF: Fn(&RequestI) -> EventPayload + Send + 'static,
    RequestI: for<'serde> Deserialize<'serde> + Send + 'static,
    RequestF: FnOnce(Arc<Agent>, HashMap<String, PathBuf>, EventMetadata, RequestI) -> RequestR
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
        .log(&format!("Started `{request_key_expr}` service."))
        .await?;
    let (response_tx, mut response_rx) = mpsc::channel(100);

    let mut services = JoinSet::new();
    services.spawn({
        let inner_agent = Arc::clone(&agent);
        async move {
            let tasks = TaskTracker::new();
            let subscriber = inner_agent
                .client
                .session
                .declare_subscriber(format!(
                    "group/{}/{}",
                    inner_agent.client.group, request_key_expr
                ))
                .await
                .context(selector::AgentCommunicationFailure {})?;
            while let Ok(sample) = subscriber.recv_async().await {
                println!(
                    "Received message on key expression: {}",
                    sample.key_expr().as_str(),
                );

                println!(
                    "Received payload: {:?}",
                    RE_PODJOB_ACTION.captures(sample.key_expr().as_str())
                );

                if let (Ok(input), Some(metadata)) = (
                    serde_json::from_slice::<RequestI>(&sample.payload().to_bytes()),
                    RE_PODJOB_ACTION.captures(sample.key_expr().as_str()),
                ) {
                    let inner_response_tx = response_tx.clone();
                    let event_metadata = EventMetadata {
                        group: metadata["group"].to_string(),
                        host: metadata["host"].to_string(),
                        subgroup: metadata["pod_job_hash"].to_string(),
                    };
                    let _event_payload = event_classifier(&input);
                    println!("Sending it to request task.");
                    tasks.spawn({
                        let inner_request_task = request_task.clone();
                        let inner_inner_agent = Arc::clone(&inner_agent);
                        let inner_namespace_lookup = namespace_lookup.clone();
                        async move {
                            inner_request_task(
                                inner_inner_agent,
                                inner_namespace_lookup,
                                event_metadata,
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
            Ok(())
        }
    });
    services.spawn(async move {
        while let Some(content) = response_rx.recv().await {
            response_task(Arc::clone(&agent.client), content?).await?;
        }
        Ok(())
    });

    services
        .join_next()
        .await
        .context(selector::NoRemainingServices {})??
}
