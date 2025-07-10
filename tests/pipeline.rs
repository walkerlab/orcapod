#![allow(clippy::panic_in_result_fn, clippy::unwrap_used, reason = "test code")]
//! Tests for pipeline creation functionality.
//!
//! This module contains tests that verify the correct creation of pipelines
//! using the `pipeline` fixture. The tests ensure that the pipeline creation
//! process completes successfully and outputs the expected results.

pub mod fixture;
use std::vec;

use fixture::pipeline;
use orcapod::uniffi::{error::Result, model::Annotation};

use crate::fixture::pipeline_job;

#[test]
fn creation() -> Result<()> {
    // This test checks if the pipeline can be created successfully.
    let pipeline = pipeline()?;

    assert_eq!(
        pipeline.annotation,
        Some(Annotation {
            name: "Example Pipeline".to_owned(),
            description: "This is an example pipeline. of A -> B -> C".to_owned(),
            version: "1.0.0".to_owned(),
        }),
        "Pipeline annotation does not match expected values."
    );

    // Check if kernel lut is accurate
    // Expected behavior is the kernel_lut should only contain unique kernels
    // so graph of 5, and 4 kernels due to the mapping being repeated
    assert_eq!(
        pipeline.kernel_lut.len(),
        4,
        "Kernel LUT should have exactly 4 entries."
    );

    Ok(())
}

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

    assert_eq!(pipeline.get_parents_for_node(node_key).count(), 0);
    Ok(())
}

#[test]
fn get_input_spec() -> Result<()> {
    let pipeline = pipeline()?;

    assert_eq!(pipeline.get_input_spec()?, vec!["input_text"]);
    Ok(())
}

#[test]
fn get_output_spec() -> Result<()> {
    let pipeline = pipeline()?;

    assert_eq!(pipeline.get_output_spec()?, vec!["output_text"]);
    Ok(())
}

#[test]
fn pipeline_job_creation() -> Result<()> {
    let pipeline_job = pipeline_job()?;

    assert_eq!(
        pipeline_job.annotation,
        Some(Annotation {
            name: "Example Pipeline Job".to_owned(),
            description: "This is an example pipeline job.".to_owned(),
            version: "1.0.0".to_owned(),
        })
    );

    Ok(())
}
