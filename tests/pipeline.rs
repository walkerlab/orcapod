#![allow(clippy::missing_docs_in_private_items, reason = "test code")]
//! Tests for pipeline creation functionality.
//!
//! This module contains tests that verify the correct creation of pipelines
//! using the `pipeline` fixture. The tests ensure that the pipeline creation
//! process completes successfully and outputs the expected results.
mod fixture;

use fixture::{pipeline, pipeline_job};
use orcapod::uniffi::error::Result;

#[test]
fn pipeline_creation() -> Result<()> {
    pipeline()?;
    Ok(())
}

#[test]
fn root_nodes() -> Result<()> {
    let pipeline = pipeline()?;
    let root_nodes = pipeline.get_root_nodes().collect::<Vec<_>>();
    assert_eq!(root_nodes.len(), 1);
    Ok(())
}

#[test]
fn get_leaf_nodes() -> Result<()> {
    let pipeline = pipeline()?;
    let leaf_nodes = pipeline.get_leaf_nodes().collect::<Vec<_>>();
    assert_eq!(leaf_nodes.len(), 1);
    Ok(())
}

#[test]
fn get_parents_key_for_node() -> Result<()> {
    let pipeline = pipeline()?;
    let node_key = pipeline.get_root_nodes().next().unwrap();
    let parents_keys = pipeline
        .get_parents_key_for_node(node_key)
        .collect::<Vec<_>>();
    assert_eq!(parents_keys.len(), 0);
    Ok(())
}

#[test]
fn pipeline_job_creation() -> Result<()> {
    let pipeline_job = pipeline_job()?;
    Ok(())
}

fn pipeline_run() -> Result<()> {
    let pipeline_job = pipeline_job()?;
    Ok(())
}
