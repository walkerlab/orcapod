#![allow(clippy::missing_docs_in_private_items, reason = "test code")]
//! Tests for pipeline creation functionality.
//!
//! This module contains tests that verify the correct creation of pipelines
//! using the `pipeline` fixture. The tests ensure that the pipeline creation
//! process completes successfully and outputs the expected results.
mod fixture;

use fixture::pipeline;
use orcapod::uniffi::error::Result;

#[test]
fn pipeline_creation() -> Result<()> {
    pipeline()?;
    Ok(())
}
