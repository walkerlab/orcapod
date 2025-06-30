#![expect(missing_docs, clippy::panic_in_result_fn, reason = "OK in tests.")]

pub mod fixture;
use fixture::{TestDirs, container_image_style, pod_job_style};
use orcapod::{
    core::{crypto::hash_buffer, model::to_yaml},
    uniffi::{
        error::Result,
        model::URI,
        orchestrator::{ImageKind, Orchestrator, PodRun, Status, docker::LocalDockerOrchestrator},
    },
};
use std::{collections::HashMap, ops::Deref as _, path::PathBuf, sync::Arc};
fn execute_wrapper<T>(test_fn: T) -> Result<()>
where
    T: Fn(&HashMap<String, PathBuf>, &LocalDockerOrchestrator) -> Result<()>,
{
    let test_dirs = TestDirs::new(&HashMap::from([(
        "default".to_owned(),
        Some("./tests/extra/data/"),
    )]))?;
    let namespace_lookup = test_dirs.namespace_lookup();
    let orchestrator = LocalDockerOrchestrator::new()?;

    test_fn(&namespace_lookup, &orchestrator)
}

fn basic_test(
    pod_run: &PodRun,
    expected_command: &str,
    orchestrator: &impl Orchestrator,
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
            .filter(|run| **run == *pod_run)
            .map(|run| Ok(orchestrator.get_info_blocking(run)?.command))
            .collect::<Result<Vec<_>>>()?,
        vec![expected_command],
        "Unexpected list."
    );
    // await result
    let pod_result_1 = orchestrator.get_result_blocking(pod_run)?;
    assert_eq!(
        orchestrator.get_info_blocking(pod_run)?.status,
        Status::Completed,
        "Unexpected state."
    );
    assert_eq!(
        orchestrator
            .list_blocking()?
            .iter()
            .filter(|run| **run == *pod_run)
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
        !orchestrator.list_blocking()?.contains(pod_run),
        "Unexpected container remains."
    );
    // try getting info of a purged pod run
    assert!(
        orchestrator
            .get_info_blocking(pod_run)
            .is_err_and(|error| error.is_purged_pod_run() && !format!("{error:?}").is_empty()),
        "Did not raise a purged pod run error."
    );
    Ok(())
}

#[test]
fn offline_container_image_basic() -> Result<()> {
    execute_wrapper(|namespace_lookup, orchestrator| {
        let container_image_relative_location = "container_images/style-transfer/image.tar.gz";
        let container_image_kind = ImageKind::Tarball(URI {
            namespace: "default".to_owned(),
            path: PathBuf::from(container_image_relative_location),
        });
        let _container_image = container_image_style(
            namespace_lookup["default"].join(container_image_relative_location),
        )?;
        let mut pod_job = pod_job_style(namespace_lookup)?;
        pod_job.env_vars = Some(HashMap::from([("DELAY".to_owned(), "1".to_owned())]));

        basic_test(
            &orchestrator.start_with_altimage_blocking(
                namespace_lookup,
                &pod_job,
                &container_image_kind,
            )?,
            &pod_job.pod.command,
            orchestrator,
        )?;
        Ok(())
    })
}

#[test]
fn remote_container_image_basic() -> Result<()> {
    execute_wrapper(|namespace_lookup, orchestrator| {
        let mut pod_job = pod_job_style(namespace_lookup)?;
        let mut pod = pod_job.pod.deref().clone();
        pod.image = "alpine:3.14".to_owned();
        pod.command = "sleep 5".to_owned();
        pod.input_spec = HashMap::new();
        pod_job.pod = Arc::new(pod);
        pod_job.input_packet = HashMap::new();

        basic_test(
            &orchestrator.start_blocking(namespace_lookup, &pod_job)?,
            &pod_job.pod.command,
            orchestrator,
        )?;
        Ok(())
    })
}

#[test]
fn logs() -> Result<()> {
    execute_wrapper(|namespace_lookup, orchestrator| {
        let mut pod_job = pod_job_style(namespace_lookup)?;

        // Update pod
        let mut pod = pod_job.pod.deref().clone();
        pod.image = "alpine:3.14".to_owned();
        pod.command = "echo hi1".to_owned();
        pod.input_spec = HashMap::new();
        // Update the hash
        pod.hash = String::new();
        pod.hash = hash_buffer(to_yaml(&pod)?);

        // Update pod job
        pod_job.pod = pod.into();
        pod_job.input_packet = HashMap::new();
        // Update the hash
        pod_job.hash = String::new();
        pod_job.hash = hash_buffer(to_yaml(&pod_job)?);

        let pod_run = orchestrator.start_blocking(namespace_lookup, &pod_job)?;
        let pod_result = orchestrator.get_result_blocking(&pod_run)?;

        assert_eq!(
            pod_result.status,
            Status::Completed,
            "Pod status is not completed"
        );

        assert_eq!(orchestrator.get_logs_blocking(&pod_run)?, "hi1\n");
        assert_eq!(orchestrator.get_result_blocking(&pod_run)?.logs, "hi1\n");

        orchestrator.delete_blocking(&pod_run)?;

        assert!(
            !orchestrator.list_blocking()?.contains(&pod_run),
            "Unexpected container remains."
        );

        Ok(())
    })
}
