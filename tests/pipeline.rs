#![allow(
    clippy::missing_docs_in_private_items,
    clippy::panic_in_result_fn,
    clippy::unwrap_used,
    reason = "test code"
)]
//! Tests for pipeline creation functionality.
//!
//! This module contains tests that verify the correct creation of pipelines
//! using the `pipeline` fixture. The tests ensure that the pipeline creation
//! process completes successfully and outputs the expected results.
mod fixture;

use fixture::{pipeline, pipeline_job};
use orcapod::core::pipeline_runner::docker::DockerPipelineRunner;
use orcapod::uniffi::error::Result;
use tokio::runtime::Runtime;

#[test]
fn root_nodes() -> Result<()> {
    let pipeline = pipeline()?;

    assert_eq!(pipeline.get_root_nodes().count(), 1);
    Ok(())
}

#[test]
fn get_leaf_nodes() -> Result<()> {
    let pipeline = pipeline()?;

    assert_eq!(pipeline.get_leaf_nodes().count(), 1);
    Ok(())
}

#[test]
fn get_parents_key_for_node() -> Result<()> {
    let pipeline = pipeline()?;
    let node_key = pipeline.get_root_nodes().next().unwrap();
    println!("{:?}", pipeline.graph);
    println!("node_key: {}", node_key);

    assert_eq!(pipeline.get_parents_key_for_node(node_key).count(), 0);
    Ok(())
}

#[test]
fn pipeline_job_creation() -> Result<()> {
    let pipeline_job = pipeline_job()?;
    Ok(())
}

/// Pipeline Runner Tests
/// This module contains tests for the pipeline runner functionality.
#[test]
fn pipeline_run() -> Result<()> {
    let pipeline_job = pipeline_job()?;

    let mut docker_pipeline_runner = DockerPipelineRunner::new();

    Ok(())
}
