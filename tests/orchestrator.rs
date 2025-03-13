#![expect(
    clippy::expect_used,
    missing_docs,
    clippy::panic_in_result_fn,
    clippy::panic,
    reason = "OK in tests."
)]

pub mod fixture;
use colored::Colorize as _;
use core::panic;
use fixture::{
    add_storage, container_image_style, pod_job_style, store_fixture, store_map_fixture, TestStore,
    TestStoredModel,
};
use orcapod::{
    error::Result,
    model::{PodJob, StoreMap},
    orchestrator::{docker::LocalDockerOrchestrator, ImageKind, Orchestrator as _, PodRun, Status},
};
use std::{
    collections::{BTreeMap, HashMap},
    thread::sleep,
    time::Duration,
};
use tempfile::TempDir;

fn setup<'store>(
    store: &'store TestStore,
    store_map: &StoreMap,
) -> Result<(TestStoredModel<'store, PodJob>, LocalDockerOrchestrator)> {
    Ok((
        add_storage(pod_job_style(store_map)?, store)?,
        LocalDockerOrchestrator::new()?,
    ))
}

fn basic_test(
    orchestrator: &LocalDockerOrchestrator,
    pod_run: &PodRun,
    expected_command: String,
) -> Result<()> {
    assert_eq!(
        orchestrator.get_info_blocking(pod_run)?.status,
        Status::Running,
        "Unexpected state."
    );
    assert_eq!(
        orchestrator
            .list_blocking()?
            .iter()
            .map(|run| Ok(orchestrator.get_info_blocking(run)?.command))
            .collect::<Result<Vec<_>>>()?,
        vec![expected_command.clone()],
        "Unexpected list."
    );

    let pod_result_1 = orchestrator.get_result_blocking(pod_run)?;
    assert_eq!(
        orchestrator.get_info_blocking(pod_run)?.status,
        Status::Completed,
        "Pod error out with {}",
        pod_result_1.logs
    );
    assert_eq!(
        orchestrator
            .list_blocking()?
            .iter()
            .map(|run| Ok(orchestrator.get_info_blocking(run)?.command))
            .collect::<Result<Vec<_>>>()?,
        vec![expected_command],
        "Unexpected list."
    );
    assert_eq!(
        pod_result_1.assigned_name, pod_run.assigned_name,
        "Unexpected name."
    );
    // try generating result again
    let pod_result_2 = orchestrator.get_result_blocking(pod_run)?;
    assert_eq!(pod_result_1, pod_result_2, "Pod results don't match.");
    // test delete
    orchestrator.delete_blocking(pod_run)?;
    assert!(
        orchestrator.list_blocking()?.is_empty(),
        "Unexpected container remains."
    );

    assert!(
        orchestrator
            .get_info_blocking(pod_run)
            .expect_err("Unexpectedly succeeded.")
            .is_purged_pod_run(),
        "Returned a different OrcaError than one expected when getting info of a purged pod run."
    );
    Ok(())
}

#[test]
fn offline_container_image_basic() -> Result<()> {
    let store = store_fixture(None)?;
    let store_map = store_map_fixture()?;
    let (mut stored_pod_job, orchestrator) = setup(&store, &store_map)?;

    // Create temp dir to store image
    let temp_dir = TempDir::new()?;

    let container_image_path = temp_dir
        .path()
        .join("container_images/style-transfer/image.tar.gz");

    let _container_image = container_image_style(&container_image_path)?;

    stored_pod_job.model.env_vars = Some(HashMap::from([("DELAY".to_owned(), "5".to_owned())]));
    let container_image_kind = ImageKind::Tarball(container_image_path);
    let pod_run = orchestrator.start_with_altimage_blocking(
        &stored_pod_job.model,
        &container_image_kind,
        &store_map,
    )?;

    basic_test(
        &orchestrator,
        &pod_run,
        stored_pod_job.model.pod.command.clone(),
    )
}

#[test]
fn expect_pod_start_fail() -> Result<()> {
    let store = store_fixture(None)?;
    let store_map = store_map_fixture()?;
    let (mut stored_pod_job, orchestrator) = setup(&store, &store_map)?;

    stored_pod_job.model.pod.image = "alpine:3.14".to_owned();
    match orchestrator.start_blocking(&stored_pod_job.model, &store_map) {
        Ok(_) => panic!("Pod was launched successfully when it should have failed."),
        Err(err) => assert_eq!(
            err.to_string(),
            format!("{}{}", "Fail to start pod with error: ".bright_red(),
                "Docker responded with status code 400: failed to create task for container: failed to create shim task: OCI runtime create failed: runc create failed: unable to start container process: error during container init: exec: \"python\": executable file not found in $PATH: unknown".bright_cyan()
            )
        ),
    }

    Ok(())
}

#[test]
fn expect_pod_run_fail() -> Result<()> {
    let store = store_fixture(None)?;
    let store_map = store_map_fixture()?;
    let (mut stored_pod_job, orchestrator) = setup(&store, &store_map)?;

    stored_pod_job.model.pod.image = "alpine:3.14".to_owned();
    stored_pod_job.model.pod.command = "sleep 1 && exit 1".to_owned();
    stored_pod_job.model.pod.input_stream_map = BTreeMap::new();
    stored_pod_job.model.input_stream_map = BTreeMap::new();

    // Start job and sleep for a few second ensuring the job has time to fail
    let pod_run = orchestrator.start_blocking(&stored_pod_job.model, &store_map)?;
    sleep(Duration::from_secs(5));

    // Should be in failed state
    assert_eq!(
        orchestrator.get_info_blocking(&pod_run)?.status,
        Status::Failed(1),
        "Unexpected state."
    );

    Ok(())
}

#[test]
fn remote_container_image_basic() -> Result<()> {
    let store = store_fixture(None)?;
    let store_map = store_map_fixture()?;
    let (mut stored_pod_job, orchestrator) = setup(&store, &store_map)?;

    stored_pod_job.model.pod.image = "alpine:3.14".to_owned();
    stored_pod_job.model.pod.command = "sleep 5".to_owned();
    stored_pod_job.model.pod.input_stream_map = BTreeMap::new();
    stored_pod_job.model.input_stream_map = BTreeMap::new();
    let pod_run = orchestrator.start_blocking(&stored_pod_job.model, &store_map)?;
    basic_test(
        &orchestrator,
        &pod_run,
        stored_pod_job.model.pod.command.clone(),
    )
}
