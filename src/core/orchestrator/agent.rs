use crate::uniffi::{
    error::{OrcaError, Result, selector},
    orchestrator::agent::{Agent, AgentClient},
};
use chrono::{DateTime, Utc};
use futures_util::future::FutureExt as _;
use itertools::Itertools as _;
use serde::{Deserialize, Serialize};
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

pub fn extract_metadata(key_expr: &str) -> HashMap<String, String> {
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
pub async fn start_service<
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
                println!("asdhfjkashdfkj{:?}", event_metadata);
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
