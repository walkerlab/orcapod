#![expect(
    missing_docs,
    clippy::panic_in_result_fn,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "OK in tests."
)]
pub mod fixture;

// Example for a local module:
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use orcapod::{
    core::pipeline_runner::DockerPipelineRunner,
    uniffi::{
        error::Result,
        model::pipeline::PipelineStatus,
        orchestrator::{agent::Agent, docker::LocalDockerOrchestrator},
    },
};
use tokio::fs::read_to_string;

use crate::fixture::TestDirs;
use fixture::pipeline_job;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn basic_run() -> Result<()> {
    // Create the test_dir and get the namespace lookup
    let test_dirs = TestDirs::new(&HashMap::from([(
        "default".to_owned(),
        Some("./tests/extra/data/"),
    )]))?;
    let namespace_lookup = test_dirs.namespace_lookup();

    // Create and agent and start it (temporary for now, will be merge later)
    let agent = Arc::new(Agent::new(
        "test:basic_run".to_owned(),
        "localhost".to_owned(),
        Arc::new(LocalDockerOrchestrator::new().unwrap()),
    )?);

    let agent_inner = Arc::clone(&agent);
    let namespace_lookup_inner = namespace_lookup.clone();
    tokio::spawn(async move {
        agent_inner
            .start(&namespace_lookup_inner, None)
            .await
            .unwrap();
    });

    let pipeline_job = pipeline_job(&namespace_lookup)?;

    // Create the runner
    let mut runner = DockerPipelineRunner::new(agent);

    let pipeline_run = runner
        .start(pipeline_job, "default", &namespace_lookup)
        .await?;

    // Wait for the pipeline run to complete
    let pipeline_result = runner.get_result(&pipeline_run).await?;

    // Check the output packet content
    assert_eq!(pipeline_result.output_packets["output"].len(), 4);

    // Check the status
    assert_eq!(pipeline_result.status, PipelineStatus::Succeeded);

    // Get all the output file content and read them in
    let mut output_content = HashSet::new();

    for output_packet in &pipeline_result.output_packets["output"] {
        output_content
            .insert(read_to_string(&output_packet.to_path_buf(&namespace_lookup)?[0]).await?);
    }

    // Check if the output_content matches
    assert_eq!(
        output_content,
        HashSet::from([
            "Where is the black cat playing\n".to_owned(),
            "Where is the black cat hiding\n".to_owned(),
            "Where is the tabby cat playing\n".to_owned(),
            "Where is the tabby cat hiding\n".to_owned(),
        ])
    );

    Ok(())
}
