#![expect(missing_docs, reason = "OK in tests.")]

use orcapod::uniffi::{
    error::Result,
    orchestrator::{agent::Agent, docker::LocalDockerOrchestrator},
};
use std::{sync::Arc, time::Duration};
use tokio::{self, time::sleep as async_sleep};

#[test]
fn simple() -> Result<()> {
    let _agent = Agent::new(
        "test".to_owned(),
        "alpha".to_owned(),
        Arc::new(LocalDockerOrchestrator::new()?),
    )?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn complex() -> Result<()> {
    let _agent = Agent::new(
        "test".to_owned(),
        "alpha".to_owned(),
        Arc::new(LocalDockerOrchestrator::new()?),
    )?;

    async_sleep(Duration::from_secs(1)).await;
    Ok(())
}
