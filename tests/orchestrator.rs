#![expect(
    clippy::expect_used,
    missing_docs,
    clippy::panic_in_result_fn,
    reason = "OK in tests."
)]

pub mod fixture;
use fixture::{
    add_storage, container_image_style, pod_job_style, store_fixture, store_map_fixture, TestStore,
    TestStoredModel,
};
use orcapod::{
    error::Result,
    model::{PodJob, StoreMap},
    orchestrator::{docker::LocalDockerOrchestrator, ImageKind, Orchestrator, PodRun, Status},
    store::{filestore::LocalFileStore, DataStore, ModelStore as _},
};
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
};

fn setup<'store>(
    store: &'store TestStore,
    store_map: &StoreMap<impl DataStore>,
) -> Result<(TestStoredModel<'store, PodJob>, LocalDockerOrchestrator)> {
    Ok((
        add_storage(pod_job_style(store_map)?, store)?,
        LocalDockerOrchestrator::new(
            store
                .get_directory()
                .join(LocalFileStore::DEFAULT_DATA_NAMESPACE),
        )?,
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
    // assert_eq!(
    //     orchestrator
    //         .list_blocking()?
    //         .iter()
    //         .map(|run| Ok(orchestrator.get_info_blocking(run)?.command))
    //         .collect::<Result<Vec<_>>>()?,
    //     vec![expected_command.clone()],
    //     "Unexpected list."
    // );
    // await result
    let pod_result_1 = orchestrator.get_result_blocking(pod_run)?;
    assert_eq!(
        orchestrator.get_info_blocking(pod_run)?.status,
        Status::Completed,
        "Pod error out with {}",
        pod_result_1.logs
    );
    // assert_eq!(
    //     orchestrator
    //         .list_blocking()?
    //         .iter()
    //         .map(|run| Ok(orchestrator.get_info_blocking(run)?.command))
    //         .collect::<Result<Vec<_>>>()?,
    //     vec![expected_command],
    //     "Unexpected list."
    // );
    assert_eq!(
        pod_result_1.assigned_name, pod_run.assigned_name,
        "Unexpected name."
    );
    // try generating result again
    let pod_result_2 = orchestrator.get_result_blocking(pod_run)?;
    assert_eq!(pod_result_1, pod_result_2, "Pod results don't match.");
    // test delete
    orchestrator.delete_blocking(pod_run)?;
    // assert!(
    //     orchestrator.list_blocking()?.is_empty(),
    //     "Unexpected container remains."
    // );
    // try getting info of a purged pod run
    // assert!(
    //     orchestrator
    //         .get_info_blocking(pod_run)
    //         .expect_err("Unexpectedly succeeded.")
    //         .is_purged_pod_run(),
    //     "Returned a different OrcaError than one expected when getting info of a purged pod run."
    // );
    Ok(())
}

#[test]
fn offline_container_image_basic() -> Result<()> {
    let store = store_fixture(None)?;
    let store_map = store_map_fixture()?;
    let (mut stored_pod_job, orchestrator) = setup(&store, &store_map)?;
    println!("{stored_pod_job:?}");
    let container_image_relative_location =
        "container_images/style-transfer/image.tar.gz".to_owned();
    let _container_image = container_image_style(
        store
            .get_directory()
            .join(LocalFileStore::DEFAULT_DATA_NAMESPACE)
            .join(container_image_relative_location.clone()),
    )?;

    stored_pod_job.model.env_vars = Some(HashMap::from([("DELAY".to_owned(), "5".to_owned())]));
    let container_image_kind = ImageKind::Tarball(PathBuf::from(container_image_relative_location));
    let pod_run =
        orchestrator.start_with_altimage_blocking(&stored_pod_job.model, &container_image_kind)?;
    basic_test(
        &orchestrator,
        &pod_run,
        stored_pod_job.model.pod.command.clone(),
    )
}

#[test]
fn remote_container_image_basic() -> Result<()> {
    let store = store_fixture(None)?;
    let store_map = store_map_fixture()?;
    let (mut stored_pod_job, orchestrator) = setup(&store, &store_map)?;

    stored_pod_job.model.pod.image = "alpine:3.14".to_owned();
    stored_pod_job.model.pod.command = "sleep 5".to_owned();
    stored_pod_job.model.pod.input_stream_map = BTreeMap::new();
    stored_pod_job.model.input_stream_mapping = BTreeMap::new();
    let pod_run = orchestrator.start_blocking(&stored_pod_job.model)?;
    basic_test(
        &orchestrator,
        &pod_run,
        stored_pod_job.model.pod.command.clone(),
    )
}
