#![expect(clippy::expect_used, reason = "Expect OK in tests.")]
#![expect(clippy::panic_in_result_fn, reason = "Panics OK in tests.")]

pub mod fixture;
use fixture::{add_storage, container_image_style, pod_job_style, store_test};
use orcapod::{
    error::Result,
    orchestrator::{docker::LocalDockerOrchestrator, ImageKind, PodRunAPI, RunState, API},
    store::{filestore::LocalFileStore, Store},
};
use std::collections::HashMap;

#[test]
fn basic() -> Result<()> {
    // setup
    let store = store_test(None, true)?;
    let container_image_location = store
        .get_directory()
        .join(LocalFileStore::DEFAULT_DATA_NAMESPACE)
        .join("container_images/style-transfer/image.tar.gz");
    let _container_image = container_image_style(&container_image_location)?;
    let pod_job = pod_job_style(&store.store)?;
    let mut stored_pod_job = add_storage(pod_job, &store)?;
    let orchestrator = LocalDockerOrchestrator::new(
        store
            .get_directory()
            .join(LocalFileStore::DEFAULT_DATA_NAMESPACE),
    )?;
    // start in background
    stored_pod_job.model.env_vars = Some(HashMap::from([("DELAY".to_owned(), "5".to_owned())]));
    let container_image_kind = ImageKind::Tarball(container_image_location);
    let pod_run = orchestrator.start_with_altimage(&stored_pod_job.model, &container_image_kind)?;
    assert_eq!(
        pod_run.get_info()?.state,
        RunState::Running,
        "Unexpected state."
    );
    assert_eq!(
        orchestrator
            .list()?
            .iter()
            .map(PodRunAPI::get_info)
            .map(|run_info| Ok(run_info?.command))
            .collect::<Result<Vec<_>>>()?,
        vec!["python /run.py".to_owned()],
        "Unexpected list."
    );
    // await result
    let pod_result_1 = pod_run.get_result()?;
    assert_eq!(
        pod_run.get_info()?.state,
        RunState::Completed,
        "Unexpected state."
    );
    assert_eq!(
        orchestrator
            .list()?
            .iter()
            .map(PodRunAPI::get_info)
            .map(|run_info| Ok(run_info?.command))
            .collect::<Result<Vec<_>>>()?,
        vec!["python /run.py".to_owned()],
        "Unexpected list."
    );
    assert_eq!(
        pod_result_1.assigned_name,
        pod_run.get_info()?.name,
        "Unexpected name."
    );
    // try generating result again
    let pod_result_2 = pod_run.get_result()?;
    assert_eq!(pod_result_1, pod_result_2, "Pod results don't match.");
    // test delete
    orchestrator.delete(&pod_run)?;
    assert!(
        orchestrator.list()?.is_empty(),
        "Unexpected container remains."
    );
    // try getting result of a purged pod run
    assert!(
        pod_run
            .get_result()
            .expect_err("Unexpectedly succeeded.")
            .is_purged_pod_run(),
        "Returned a different OrcaError than one expected when querying a purged pod run."
    );
    // try deleting a purged pod run
    assert!(
        orchestrator
            .delete(&pod_run)
            .expect_err("Unexpectedly succeeded.")
            .is_purged_pod_run(),
        "Returned a different OrcaError than one expected when querying a purged pod run."
    );
    Ok(())
}
