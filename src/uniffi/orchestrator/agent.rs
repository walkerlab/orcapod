use crate::{
    core::orchestrator::ASYNC_RUNTIME,
    uniffi::{
        error::{OrcaError, Result, selector},
        orchestrator::{Orchestrator, docker::LocalDockerOrchestrator},
    },
};
use derive_more::Display;
use getset::CloneGetters;
use snafu::ResultExt as _;
use std::sync::Arc;
use uniffi;
use zenoh;

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
            session: ASYNC_RUNTIME.block_on(async {
                Ok::<_, OrcaError>(
                    zenoh::open(zenoh::Config::default())
                        .await
                        .context(selector::AgentCommunicationFailure {})?,
                )
            })?,
        })
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

#[uniffi::export]
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
}
