#![expect(
    clippy::expect_used,
    missing_docs,
    clippy::panic_in_result_fn,
    clippy::panic,
    clippy::unwrap_used,
    reason = "OK in tests."
)]

pub mod fixture;
use fixture::{TestContainerImage, TestDirs, container_image_style, pod_job_style};
use orcapod::uniffi::{
    error::{Kind, Result},
    model::OrcaPath,
    orchestrator::{ImageKind, Orchestrator as _, PodRun, Status, docker::LocalDockerOrchestrator},
};
use std::{collections::HashMap, path::PathBuf};

fn basic_test<T>(start: T) -> Result<()>
where
    T: Fn(
        &HashMap<String, PathBuf>,
        &LocalDockerOrchestrator,
    ) -> Result<(PodRun, String, Option<TestContainerImage>)>,
{
    let test_dirs = TestDirs::new(&HashMap::from([(
        "default".to_owned(),
        Some("./tests/data/"),
    )]))?;
    let namespace_lookup = test_dirs.namespace_lookup();
    let orchestrator = LocalDockerOrchestrator::new()?;
    let (pod_run, expected_command, _container_image) = start(&namespace_lookup, &orchestrator)?;
    assert_eq!(
        orchestrator.get_info_blocking(&pod_run)?.status,
        Status::Running,
        "Unexpected state."
    );
    assert_eq!(
        orchestrator
            .list_blocking()?
            .iter()
            .filter(|run| **run == pod_run)
            .map(|run| Ok(orchestrator.get_info_blocking(run)?.command))
            .collect::<Result<Vec<_>>>()?,
        vec![expected_command.clone()],
        "Unexpected list."
    );
    // await result
    let pod_result_1 = orchestrator.get_result_blocking(&pod_run)?;
    assert_eq!(
        orchestrator.get_info_blocking(&pod_run)?.status,
        Status::Completed,
        "Unexpected state."
    );
    assert_eq!(
        orchestrator
            .list_blocking()?
            .iter()
            .filter(|run| **run == pod_run)
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
    let pod_result_2 = orchestrator.get_result_blocking(&pod_run)?;
    assert_eq!(pod_result_1, pod_result_2, "Pod results don't match.");
    // test delete
    orchestrator.delete_blocking(&pod_run)?;
    assert!(
        !orchestrator
            .list_blocking()?
            .iter()
            .any(|run| *run == pod_run),
        "Unexpected container remains."
    );
    // try getting info of a purged pod run
    assert!(
        orchestrator
            .get_info_blocking(&pod_run)
            .expect_err("Unexpectedly succeeded.")
            .is_purged_pod_run(),
        "Returned a different OrcaError than one expected when getting info of a purged pod run."
    );
    Ok(())
}

#[test]
fn offline_container_image_basic() -> Result<()> {
    basic_test(|namespace_lookup, orchestrator| {
        let container_image_relative_location = "container_images/style-transfer/image.tar.gz";
        let container_image_kind = ImageKind::Tarball(OrcaPath {
            namespace: "default".to_owned(),
            path: PathBuf::from(container_image_relative_location),
        });
        let container_image = container_image_style(
            namespace_lookup["default"].join(container_image_relative_location),
        )?;
        let mut pod_job = pod_job_style(namespace_lookup)?;
        pod_job.env_vars = Some(HashMap::from([("DELAY".to_owned(), "1".to_owned())]));
        Ok((
            orchestrator.start_with_altimage_blocking(
                namespace_lookup,
                &pod_job,
                &container_image_kind,
            )?,
            pod_job.pod.command,
            Some(container_image),
        ))
    })
}

#[test]
fn remote_container_image_basic() -> Result<()> {
    basic_test(|namespace_lookup, orchestrator| {
        let mut pod_job = pod_job_style(namespace_lookup)?;
        pod_job.pod.image = "alpine:3.14".to_owned();
        pod_job.pod.command = "sleep 1".to_owned();
        pod_job.pod.input_stream = HashMap::new();
        pod_job.input_stream = HashMap::new();
        Ok((
            orchestrator.start_blocking(namespace_lookup, &pod_job)?,
            pod_job.pod.command,
            None,
        ))
    })
}

fn execute_wrapper<T>(test_fn: T) -> Result<()>
where
    T: Fn(&HashMap<String, PathBuf>, &LocalDockerOrchestrator) -> Result<()>,
{
    let test_dirs = TestDirs::new(&HashMap::from([(
        "default".to_owned(),
        Some("./tests/data/"),
    )]))?;
    let namespace_lookup = test_dirs.namespace_lookup();
    let orchestrator = LocalDockerOrchestrator::new()?;

    test_fn(&namespace_lookup, &orchestrator)
}

#[test]
fn command_parse() -> Result<()> {
    execute_wrapper(|namespace_lookup, orchestrator| {
        let mut pod_job = pod_job_style(namespace_lookup)?;

        pod_job.pod.image = "alpine:3.14".to_owned();
        pod_job.pod.command = r#"echo 'hi 1' && echo "hi 2""#.to_owned();
        pod_job.pod.input_stream = HashMap::new();
        pod_job.input_stream = HashMap::new();

        let pod_run = orchestrator.start_blocking(namespace_lookup, &pod_job)?;
        let pod_result = orchestrator.get_result_blocking(&pod_run)?;

        assert_eq!(
            pod_result.status,
            Status::Completed,
            "Pod status is not completed"
        );

        assert_eq!(
            pod_result.logs, "hi 1 && echo hi 2\n",
            "Logs do not match error"
        );

        orchestrator.delete_blocking(&pod_run)?;

        assert!(
            !orchestrator
                .list_blocking()?
                .iter()
                .any(|pod_run_from_list| *pod_run_from_list == pod_run),
            "Unexpected container remains."
        );

        Ok(())
    })
}

#[test]
/// Expect pod to fail due to bad command, where the expected behavior should auto delete the container and return an error
fn expect_pod_start_fail() -> Result<()> {
    execute_wrapper(|namespace_lookup, orchestrator| {
        let mut pod_job = pod_job_style(namespace_lookup)?;
        pod_job.pod.image = "alpine:3.14".to_owned();
        pod_job.pod.command = "python file_does_not_exist.py".to_owned();
        let container_name = match orchestrator.start_blocking(namespace_lookup, &pod_job) {
            Ok(_) => panic!("Pod was launched successfully when it should have failed."),
            Err(err) => {
                let Kind::FailedToStartPod { container_name, .. } = &err.0 else {
                    panic!("Unexpected error type.")
                };

                assert_eq!(
                    err.to_string(),
                    format!(
                        "Fail to start pod with container_name: {container_name} with error: Docker responded with status code 400: failed to create task for container: failed to create shim task: OCI runtime create failed: runc create failed: unable to start container process: error during container init: exec: \"python\": executable file not found in $PATH: unknown"
                    ),
                    "Error message does not match."
                );
                container_name.to_owned()
            }
        };

        // Make sure the pod has been deleted after failing to start
        let list_result = orchestrator
            .list_blocking()?
            .into_iter()
            .filter(|pod_run| pod_run.assigned_name == *container_name)
            .collect::<Vec<_>>();

        assert!(
            list_result.len() == 1,
            "List didn't return just the fail pod."
        );

        let pod_run = list_result.first().unwrap();

        // Get the pod result and make sure it is in failed state
        let pod_result = orchestrator.get_result_blocking(pod_run)?;

        assert_eq!(
            pod_result.status,
            Status::Failed(127),
            "Pod status is not failed"
        );

        assert_eq!(
            pod_result.logs,
            "failed to create task for container: failed to create shim task: OCI runtime create failed: runc create failed: unable to start container process: error during container init: exec: \"python\": executable file not found in $PATH: unknown",
            "Logs do not match"
        );

        // Clean up the pod
        orchestrator.delete_blocking(pod_run)?;

        assert!(
            !orchestrator
                .list_blocking()?
                .iter()
                .any(|run| *run == *pod_run),
            "Unexpected container remains."
        );

        Ok(())
    })
}
