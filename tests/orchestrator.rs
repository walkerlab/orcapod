use orcapod::orchestrator::{docker::LocalDockerOrchestrator, Orchestrator};
use std::error::Error;

#[test]
fn docker_smoke() -> Result<(), Box<dyn Error>> {
    let orchestrator = LocalDockerOrchestrator::new()?;
    orchestrator.start()?;
    orchestrator.list()?;
    orchestrator.delete()?;
    Ok(())
}
