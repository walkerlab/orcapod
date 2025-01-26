#![expect(clippy::panic_in_result_fn, reason = "Panics OK in tests.")]

pub mod fixture;
use fixture::{add_storage, pod_job_style, store_test};
use orcapod::{
    error::Result,
    orchestrator::{docker::LocalDockerOrchestrator, PodRunAPI, RunState, API},
    store::{filestore::LocalFileStore, Store},
};
use std::collections::HashMap;

#[test]
fn basic() -> Result<()> {
    let store = store_test(None, true)?;
    let mut pod_job = pod_job_style(&store.store, true)?;
    pod_job.env_vars = Some(HashMap::from([("DELAY".to_owned(), "0".to_owned())]));
    let stored_pod_job = add_storage(pod_job, &store)?;
    let orchestrator = LocalDockerOrchestrator::new(
        store
            .get_directory()
            .join(LocalFileStore::DEFAULT_DATA_NAMESPACE),
    )?;
    let pod_run = orchestrator.start(&stored_pod_job.model)?;
    let pod_runs = orchestrator.list()?;

    let runs = pod_runs
        .iter()
        .map(PodRunAPI::get_info)
        .collect::<Result<Vec<_>>>()?;

    assert!(pod_run.get_info()?.is_some(), "Run isn't present.");
    assert_eq!(
        runs.into_iter()
            .filter_map(|x| Some(x?.state))
            .collect::<Vec<_>>(),
        vec![RunState::Completed],
        "Unexpected run list."
    );

    orchestrator.delete(&pod_run)?;
    orchestrator.delete(&pod_run)?; // ensure repeated deletes succeed since ignored.
    Ok(())
}
