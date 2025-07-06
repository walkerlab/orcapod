use crate::uniffi::{
    error::{OrcaError, Result, selector},
    orchestrator::agent::{Agent, AgentClient},
};
use chrono::{DateTime, Utc};
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
static RE_AGENT_KEY_EXPR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        "(?x)
            ^
            group/
                (?<group>[a-z_]+)/
                    (?<action>request|success|failure)/
                        (?<model_type>[a-z_]+)/
                            (?<ref>[0-9a-f]+)/
                                .*?
                                    host/
                                        (?<host>[a-z_]+)/
                                            timestamp/
                                                (?<timestamp>.*?)
            $
            ",
    )
    .expect("Invalid PodJob action regex.")
});

#[expect(
    dead_code,
    reason = "Need to be able to initialize to pass metadata as input."
)]
#[derive(Debug)]
pub struct EventMetadata {
    pub group: String,
    pub action: String,
    pub model_type: String,
    pub r#ref: String,
    pub host: String,
    pub timestamp: DateTime<Utc>,
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
    clippy::excessive_nesting,
    clippy::let_underscore_must_use,
    reason = "`result::Result<(), SendError<_>>` is the only uncaptured result since it would mean we can't transmit results over mpsc."
)]
pub async fn start_service<RequestF, RequestI, RequestR, ResponseF, ResponseI, ResponseR>(
    agent: Arc<Agent>,
    request_key_expr: String,
    namespace_lookup: HashMap<String, PathBuf>,
    request_task: RequestF,
    response_task: ResponseF,
) -> Result<()>
where
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
                if let (Ok(input), Some(metadata)) = (
                    serde_json::from_slice::<RequestI>(&sample.payload().to_bytes()),
                    RE_AGENT_KEY_EXPR.captures(sample.key_expr().as_str()),
                ) {
                    let inner_response_tx = response_tx.clone();
                    let event_metadata = EventMetadata {
                        group: metadata["group"].to_string(),
                        action: metadata["action"].to_string(),
                        model_type: metadata["model_type"].to_string(),
                        r#ref: metadata["ref"].to_string(),
                        host: metadata["host"].to_string(),
                        timestamp: DateTime::parse_from_rfc3339(&metadata["timestamp"])?.into(),
                    };
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
