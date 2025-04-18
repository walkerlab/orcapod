#![expect(missing_docs, clippy::panic_in_result_fn, reason = "OK in tests.")]

pub mod fixture;
use fixture::{NAMESPACE_LOOKUP_READ_ONLY, pod_job_style};
use glob::glob;
use orcapod::uniffi::{
    error::{OrcaError, Result},
    orchestrator::{Orchestrator as _, docker::LocalDockerOrchestrator},
};
use serde_json;
use serde_yaml;
use std::{collections::HashMap, fs, path::PathBuf};

fn contains_debug(error: impl Into<OrcaError>) -> bool {
    !format!("{:?}", error.into()).is_empty()
}

#[test]
fn external_bollard() -> Result<()> {
    let orch = LocalDockerOrchestrator::new()?;
    let mut pod_job = pod_job_style(&NAMESPACE_LOOKUP_READ_ONLY)?;
    pod_job.pod.image = "nonexistent_image".to_owned();
    assert!(
        orch.start_blocking(&NAMESPACE_LOOKUP_READ_ONLY, &pod_job)
            .is_err_and(contains_debug),
        "Did not raise a bollard error."
    );
    Ok(())
}

#[test]
fn external_glob() {
    assert!(
        glob("a**/b").is_err_and(contains_debug),
        "Did not raise a glob error."
    );
}

#[test]
fn external_io() {
    assert!(
        fs::read_to_string("nonexistent_file.txt").is_err_and(contains_debug),
        "Did not raise an I/O error."
    );
}

#[test]
fn external_path_prefix() {
    assert!(
        PathBuf::from("/fake/path")
            .strip_prefix("/missing/path")
            .is_err_and(contains_debug),
        "Did not raise a path prefix error."
    );
}

#[test]
fn external_json() {
    assert!(
        serde_json::from_str::<HashMap<String, String>>("{").is_err_and(contains_debug),
        "Did not raise a serde json error."
    );
}

#[test]
fn external_yaml() {
    assert!(
        serde_yaml::from_str::<HashMap<String, String>>(":").is_err_and(contains_debug),
        "Did not raise a serde yaml error."
    );
}
