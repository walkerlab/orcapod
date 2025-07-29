use crate::{
    core::orchestrator::agent::{EventPayload, start_service},
    uniffi::{
        error::{OrcaError, Result, selector},
        model::pod::{PodJob, PodResult},
        orchestrator::{Orchestrator, Status, docker::LocalDockerOrchestrator},
        store::{Store as _, filestore::LocalFileStore},
    },
};
use derive_more::Display;
use futures_executor::block_on;
use futures_util::future::join_all;
use getset::CloneGetters;
use serde_json::Value;
use snafu::{OptionExt as _, ResultExt as _};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::task::JoinSet;
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
    pub(crate) session: zenoh::Session,
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
            })?,
        })
    }
    /// Submit many pod jobs to be processed in parallel.
    /// Return order will match inputs, casting outputs to `String` (since `uniffi` doesn't support sending unwrapped `Result`s).
    pub async fn submit_pod_jobs(&self, pod_jobs: Vec<Arc<PodJob>>) -> Vec<Response> {
        join_all(pod_jobs.iter().map(|pod_job| async {
            match self
                .publish(&format!("request/pod_job/{}", pod_job.hash), pod_job)
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
        while let Ok(sample) = subscriber.recv_async().await {
            let value = serde_json::from_slice::<Value>(&sample.payload().to_bytes())?;
            println!("{}: {value:#}", sample.key_expr().as_str());
        }
        Ok(())
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
    #[expect(clippy::excessive_nesting, reason = "Nesting manageable.")]
    pub async fn start(
        &self,
        namespace_lookup: &HashMap<String, PathBuf>,
        available_store: Option<Arc<LocalFileStore>>,
    ) -> Result<()> {
        let mut services = JoinSet::new();
        services.spawn(start_service(
            Arc::new(self.clone()),
            "request/pod_job/**".to_owned(),
            namespace_lookup.clone(),
            |pod_job: &PodJob| EventPayload::Request(pod_job.clone()),
            async |agent, inner_namespace_lookup, _, pod_job| {
                let pod_run = agent
                    .orchestrator
                    .start(&inner_namespace_lookup, &pod_job)
                    .await?;
                let pod_result = agent.orchestrator.get_result(&pod_run).await?;
                agent.orchestrator.delete(&pod_run).await?;
                Ok(pod_result)
            },
            async |client, pod_result| {
                let response_topic = match &pod_result.status {
                    Status::Completed => &format!("success/pod_job/{}", pod_result.pod_job.hash),
                    Status::Running | Status::Failed(_) | Status::Unset => {
                        &format!("failure/pod_job/{}", pod_result.pod_job.hash)
                    }
                };
                client.publish(response_topic, &pod_result).await
            },
        ));
        if let Some(store) = available_store {
            services.spawn(start_service(
                Arc::new(self.clone()),
                "success/pod_job/**".to_owned(),
                namespace_lookup.clone(),
                |pod_result: &PodResult| EventPayload::Success(pod_result.clone()),
                {
                    let inner_store = Arc::clone(&store);
                    async move |_, _, _, pod_result| {
                        inner_store.save_pod_result(&pod_result)?;
                        Ok(())
                    }
                },
                async |_, ()| Ok(()),
            ));
            services.spawn(start_service(
                Arc::new(self.clone()),
                "failure/pod_job/**".to_owned(),
                namespace_lookup.clone(),
                |pod_result: &PodResult| EventPayload::Failure(pod_result.clone()),
                async move |_, _, _, pod_result| {
                    store.save_pod_result(&pod_result)?;
                    Ok(())
                },
                async |_, ()| Ok(()),
            ));
        }
        services
            .join_next()
            .await
            .context(selector::NoRemainingServices {})??
    }
}
