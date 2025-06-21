#![expect(missing_docs, reason = "OK in tests.")]

use orcapod::uniffi::{
    error::Result,
    orchestrator::{
        agent::{Agent, AgentClient},
        docker::LocalDockerOrchestrator,
    },
};
use std::{sync::Arc, time::Duration};
use tokio::{self, time::sleep as async_sleep};

#[test]
fn simple() -> Result<()> {
    let (group, host) = ("test", "alpha");
    let _client = AgentClient::new(group.to_owned(), host.to_owned())?;
    let _agent = Agent::new(
        group.to_owned(),
        host.to_owned(),
        Arc::new(LocalDockerOrchestrator::new()?),
    )?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn complex() -> Result<()> {
    let (group, host) = ("test", "alpha");
    let _client = AgentClient::new(group.to_owned(), host.to_owned())?;
    let _agent = Agent::new(
        group.to_owned(),
        host.to_owned(),
        Arc::new(LocalDockerOrchestrator::new()?),
    )?;

    async_sleep(Duration::from_secs(1)).await;
    Ok(())
}
