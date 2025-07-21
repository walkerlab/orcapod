#![expect(missing_docs, reason = "OK in tests.")]

// If 'fixture' is a local module, ensure there is a 'mod fixture;' statement or a 'fixture.rs' file in the same directory or in 'tests/'.
// If 'fixture' is an external crate, add it to Cargo.toml and import as shown below.
// use fixture::pipeline_job;
pub mod fixture;

// Example for a local module:
use std::collections::HashMap;

use orcapod::uniffi::{error::Result, pipeline_runner::runner::DockerPipelineRunner};
use snafu::ResultExt;
use tokio::{task::JoinSet, time::sleep};

use crate::fixture::TestDirs;
use fixture::pipeline_job;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn basic_run() -> Result<()> {
    let pipeline_job = pipeline_job()?;

    // Create zenoh to monitor the node ready message
    let zenoh = zenoh::open(zenoh::Config::default()).await.unwrap(); // Replace with the correct error variant if needed

    tokio::spawn({
        async move {
            let sub = zenoh.declare_subscriber("**/failure").await.unwrap();
            // Receive loop ready, publish ready message
            println!("Listening for messages...");
            loop {
                match sub.recv_async().await {
                    Ok(msg) => {
                        println!(
                            "Received message: {:?}",
                            msg.payload().try_to_string().unwrap()
                        );
                    }
                    Err(err) => {
                        println!("Error receiving message: {}", err);
                        break;
                    }
                }
            }
        }
    });

    // Create the runner
    let mut runner = DockerPipelineRunner::new();

    let test_dirs = TestDirs::new(&HashMap::from([(
        "default".to_owned(),
        Some("./tests/extra/data/"),
    )]))?;
    let namespace_lookup = test_dirs.namespace_lookup();

    let pipeline_run = runner
        .start(pipeline_job, "default", &namespace_lookup)
        .await?;

    // Wait for the pipeline run to complete
    let pipeline_result = runner.get_result(&pipeline_run).await?;
    println!("{:?}", pipeline_result.output_packets);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stop() -> Result<()> {
    let pipeline_job = pipeline_job()?;

    // Create the runner
    let mut runner = DockerPipelineRunner::new();

    let test_dirs = TestDirs::new(&HashMap::from([(
        "default".to_owned(),
        Some("./tests/extra/data/"),
    )]))?;
    let namespace_lookup = test_dirs.namespace_lookup();

    let pipeline_run = runner
        .start(pipeline_job, "default", &namespace_lookup)
        .await?;

    // Abort the pipeline run
    runner.stop(&pipeline_run).await?;

    Ok(())
}
