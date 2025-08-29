#![expect(
    missing_docs,
    clippy::panic_in_result_fn,
    clippy::indexing_slicing,
    reason = "OK in tests."
)]

pub mod fixture;
use chrono::DateTime;
use dot_parser::ast::Graph as DOTGraph;
use fixture::{NAMESPACE_LOOKUP_READ_ONLY, pod_custom, pod_job_custom, pod_job_style, str_to_vec};
use glob::glob;
use orcapod::uniffi::{
    error::{OrcaError, Result},
    model::packet::PathInfo,
    orchestrator::{
        Orchestrator as _,
        agent::{AgentClient, Response},
        docker::LocalDockerOrchestrator,
    },
};
use serde_json;
use serde_yaml;
use std::{collections::HashMap, fs, ops::Deref as _, path::PathBuf, sync::Arc, time::Duration};
use tokio::{self, time::sleep as async_sleep};

fn contains_debug(error: impl Into<OrcaError>) -> bool {
    !format!("{:?}", error.into()).is_empty()
}

#[test]
fn external_bollard() -> Result<()> {
    let orch = LocalDockerOrchestrator::new()?;
    let mut pod_job = pod_job_style(&NAMESPACE_LOOKUP_READ_ONLY)?;
    let mut pod = pod_job.pod.deref().clone();
    pod.image = "nonexistent_image".to_owned();
    pod_job.pod = Arc::new(pod);
    assert!(
        orch.start_blocking(&NAMESPACE_LOOKUP_READ_ONLY, &pod_job)
            .is_err_and(contains_debug),
        "Did not raise a bollard error."
    );
    Ok(())
}

#[test]
fn external_chrono() {
    assert!(
        DateTime::parse_from_rfc3339("Whoops").is_err_and(contains_debug),
        "Did not raise a chrono error."
    );
}

#[test]
fn external_dot() {
    assert!(
        DOTGraph::try_from("graph {").is_err_and(contains_debug),
        "Did not raise a DOT error."
    );
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

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn external_tokio_task() {
    let handle = tokio::spawn(async_sleep(Duration::from_secs(60 * 60)));
    handle.abort();
    assert!(
        handle.await.is_err_and(contains_debug),
        "Did not raise a tokio task join error."
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn internal_agent_communication_failure() -> Result<()> {
    let client = AgentClient::new(
        "error_internal-agent-communication-failure".into(),
        "host".into(),
    )?;
    assert!(
        client
            .watch("oh?no".into())
            .await
            .is_err_and(contains_debug),
        "Did not raise an agent communication failure error."
    );
    Ok(())
}

#[test]
fn internal_incomplete_packet() -> Result<()> {
    assert!(
        pod_job_custom(
            &pod_custom(
                "alpine:3.14",
                &["echo".into()],
                HashMap::from([(
                    "key_1".into(),
                    PathInfo {
                        path: "/tmp/input.txt".into(),
                        match_pattern: r".*\.txt".into()
                    }
                )])
            )?,
            HashMap::new(),
            &NAMESPACE_LOOKUP_READ_ONLY
        )
        .is_err_and(contains_debug),
        "Did not raise an incomplete packet error."
    );
    Ok(())
}

#[test]
fn internal_key_missing() {
    assert!(
        pod_job_style(&HashMap::new()).is_err_and(contains_debug),
        "Did not raise a key missing error."
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn internal_start_pod_jobs() -> Result<()> {
    let client = AgentClient::new("error_internal-start-pod-jobs".into(), "host".into())?;
    let mut pod_job = pod_job_custom(
        &pod_custom("alpine:3.14", &str_to_vec("sleep 5"), HashMap::new())?,
        HashMap::new(),
        &NAMESPACE_LOOKUP_READ_ONLY,
    )?;
    pod_job.hash = "bad?hash".into();
    let responses = client.start_pod_jobs(vec![pod_job.into()]).await;
    assert!(
        responses.len() == 1,
        "Client received an unexpected number of pod job request responses."
    );
    assert!(
        match &responses[0] {
            Response::Ok => false,
            Response::Err(message) => message.contains("Agent encountered a communication error."),
        },
        "Client did not experience expected publish error."
    );
    Ok(())
}
