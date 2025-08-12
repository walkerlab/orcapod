#![expect(
    missing_docs,
    clippy::panic_in_result_fn,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "OK in tests."
)]

// If 'fixture' is a local module, ensure there is a 'mod fixture;' statement or a 'fixture.rs' file in the same directory or in 'tests/'.
// If 'fixture' is an external crate, add it to Cargo.toml and import as shown below.
// use fixture::pipeline_job;
pub mod fixture;

// Example for a local module:
use std::{collections::HashMap, sync::Arc};

use orcapod::{
    core::pipeline_runner::DockerPipelineRunner,
    uniffi::{
        error::Result,
        orchestrator::{agent::Agent, docker::LocalDockerOrchestrator},
    },
};

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

    // create a zenoh session to print out all communication message
    let session = zenoh::open(zenoh::Config::default())
        .await
        .expect("Failed to open zenoh session");

    tokio::spawn(async move {
        // Subscribe to all messages in the 'test' group
        let sub = session
            .declare_subscriber("**")
            .await
            .expect("Failed to declare subscriber");

        while let Ok(sample) = sub.recv_async().await {
            // Print the key expression and payload of each message
            println!("Received message: {}:", sample.key_expr().as_str(),);
        }
    });

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

    println!(
        "Pipeline run completed: {:?}",
        pipeline_result.output_packets
    );

    assert!(
        pipeline_result.output_packets.len() == 1,
        "Expected exactly one output packet."
    );

    Ok(())
}

// #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
// async fn stop() -> Result<()> {
//     // Create the test_dir and get the namespace lookup
//     let test_dirs = TestDirs::new(&HashMap::from([(
//         "default".to_owned(),
//         Some(
//             "./tests/extra
//         /data/",
//         ),
//     )]))?;

//     let namespace_lookup = test_dirs.namespace_lookup();

//     let pipeline_job = pipeline_job(&namespace_lookup)?;

//     // Create the runner
//     let mut runner = DockerPipelineRunner::new("test".to_owned())?;

//     let pipeline_run = runner
//         .start(pipeline_job, "default", &namespace_lookup)
//         .await?;

//     // Abort the pipeline run
//     runner.stop(&pipeline_run).await?;

//     Ok(())
// }
