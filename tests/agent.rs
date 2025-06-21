#![expect(missing_docs, reason = "OK in tests.")]

use orcapod::uniffi::{
    error::Result,
    orchestrator::{agent::Agent, docker::LocalDockerOrchestrator},
};
use std::sync::Arc;

#[test]
fn simple() -> Result<()> {
    let _agent = Agent::new(
        "test".to_owned(),
        "alpha".to_owned(),
        Arc::new(LocalDockerOrchestrator::new()?),
    )?;
    Ok(())
}
