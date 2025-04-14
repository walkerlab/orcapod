#![expect(missing_docs, clippy::expect_used, reason = "OK in tests.")]

pub mod fixture;
use fixture::{NAMESPACE_LOOKUP_READ_ONLY, pod_job_style};
use glob::glob;
use orcapod::uniffi::{
    error::{OrcaError, Result},
    orchestrator::{Orchestrator as _, docker::LocalDockerOrchestrator},
};
use serde_json;
use serde_yaml;
use std::{collections::HashMap, fmt, fs, path::PathBuf, result};

fn check<A: fmt::Debug, B: Into<OrcaError>>(value: result::Result<A, B>) -> String {
    let error: OrcaError = value.expect_err("Did not return an expected error.").into();
    format!("{error:?}")
}

#[test]
fn external_bollard() -> Result<()> {
    let orch = LocalDockerOrchestrator::new()?;
    let mut pod_job = pod_job_style(&NAMESPACE_LOOKUP_READ_ONLY)?;
    pod_job.pod.image = "nonexistent_image".to_owned();
    check(orch.start_blocking(&NAMESPACE_LOOKUP_READ_ONLY, &pod_job));
    Ok(())
}

#[test]
fn external_glob() {
    check(glob("a**/b"));
}

#[test]
fn external_io() {
    check(fs::read_to_string("nonexistent_file.txt"));
}

#[test]
fn external_path_prefix() {
    check(PathBuf::from("/fake/path").strip_prefix("/missing/path"));
}

#[test]
fn external_json() {
    check(serde_json::from_str::<HashMap<String, String>>("{"));
}

#[test]
fn external_yaml() {
    check(serde_yaml::from_str::<HashMap<String, String>>(":"));
}
