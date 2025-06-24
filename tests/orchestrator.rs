#![expect(
    missing_docs,
    clippy::panic_in_result_fn,
    clippy::panic,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "OK in tests."
)]

pub mod fixture;
use bollard::image::CreateImageOptions;
use fixture::{TestContainerImage, TestDirs, container_image_style, pod_job_style};
use futures_util::StreamExt as _;
use orcapod::uniffi::{
    error::Result,
    model::URI,
    orchestrator::{ImageKind, Orchestrator as _, PodRun, Status, docker::LocalDockerOrchestrator},
};
use std::{
    collections::HashMap, ops::Deref as _, path::PathBuf, sync::Arc, thread::sleep, time::Duration,
};
use tokio::runtime::Runtime;

fn basic_test<T>(start: T) -> Result<()>
where
    T: Fn(
        &HashMap<String, PathBuf>,
        &LocalDockerOrchestrator,
    ) -> Result<(PodRun, String, Option<TestContainerImage>)>,
{
    let test_dirs = TestDirs::new(&HashMap::from([(
        "default".to_owned(),
        Some("./tests/extra/data/"),
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
        !orchestrator.list_blocking()?.contains(&pod_run),
        "Unexpected container remains."
    );
    // try getting info of a purged pod run
    assert!(
        orchestrator
            .get_info_blocking(&pod_run)
            .is_err_and(|error| error.is_purged_pod_run() && !format!("{error:?}").is_empty()),
        "Did not raise a purged pod run error."
    );
    Ok(())
}

#[test]
fn offline_container_image_basic() -> Result<()> {
    basic_test(|namespace_lookup, orchestrator| {
        let container_image_relative_location = "container_images/style-transfer/image.tar.gz";
        let container_image_kind = ImageKind::Tarball(URI {
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
            pod_job.pod.command.clone(),
            Some(container_image),
        ))
    })
}

#[test]
fn remote_container_image_basic() -> Result<()> {
    basic_test(|namespace_lookup, orchestrator| {
        let mut pod_job = pod_job_style(namespace_lookup)?;
        let mut pod = pod_job.pod.deref().clone();
        pod.image = "alpine:3.14".to_owned();
        pod.command = "sleep 5".to_owned();
        pod.input_spec = HashMap::new();
        pod_job.pod = Arc::new(pod);
        pod_job.input_packet = HashMap::new();
        Ok((
            orchestrator.start_blocking(namespace_lookup, &pod_job)?,
            pod_job.pod.command.clone(),
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
        Some("./tests/extra/data/"),
    )]))?;
    let namespace_lookup = test_dirs.namespace_lookup();
    let orchestrator = LocalDockerOrchestrator::new()?;

    test_fn(&namespace_lookup, &orchestrator)
}

#[test]
fn command_parse() -> Result<()> {
    execute_wrapper(|namespace_lookup, orchestrator| {
        let mut pod_job = pod_job_style(namespace_lookup)?;

        let mut pod = pod_job.pod.deref().clone();
        pod.image = "alpine:3.14".to_owned();
        pod.command = r#"sh -c "echo hi1 && echo 'hi2'"""#.to_owned();
        pod.input_spec = HashMap::new();
        pod_job.pod = pod.into();

        pod_job.input_packet = HashMap::new();

        let pod_run = orchestrator.start_blocking(namespace_lookup, &pod_job)?;
        sleep(Duration::from_secs(1));
        let pod_result = orchestrator.get_result_blocking(&pod_run)?;

        assert_eq!(
            pod_result.status,
            Status::Completed,
            "Pod status is not completed"
        );

        orchestrator.delete_blocking(&pod_run)?;

        assert!(
            !orchestrator.list_blocking()?.contains(&pod_run),
            "Unexpected container remains."
        );

        Ok(())
    })
}

#[test]
/// Expect pod to fail due to bad command, where the expected behavior should auto delete the container and return an error
fn fail_at_start() -> Result<()> {
    execute_wrapper(|namespace_lookup, orchestrator| {
        let mut pod_job = pod_job_style(namespace_lookup)?;
        let mut pod = pod_job.pod.deref().clone();
        pod.image = "alpine:3.14".to_owned();
        pod.command = "python file_does_not_exist.py".to_owned();
        pod_job.pod = pod.into();

        let container_name = match orchestrator.start_blocking(namespace_lookup, &pod_job) {
            Ok(_) => panic!("Pod was launched successfully when it should have failed."),
            Err(err) => {
                assert!(err.is_failed_to_start_pod());
                err.to_string().split(' ').collect::<Vec<&str>>()[6].to_owned()
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

        // Clean up the pod
        orchestrator.delete_blocking(pod_run)?;

        assert!(
            !orchestrator.list_blocking()?.contains(pod_run),
            "Unexpected container remains."
        );

        Ok(())
    })
}

#[test]
fn fail_during_execution() -> Result<()> {
    execute_wrapper(|namespace_lookup, orchestrator| {
        let mut pod_job = pod_job_style(namespace_lookup)?;
        let mut pod = pod_job.pod.deref().clone();

        pod.image = "alpine:3.14".to_owned();
        pod.command = "python file_does_not_exist.py".to_owned();

        pod.image = "alpine:3.14".to_owned();
        pod.command = r#"bin/sh -c 'echo "hi" && bad_command'"#.to_owned();
        pod.input_spec = HashMap::new();
        pod_job.pod = pod.into();
        pod_job.input_packet = HashMap::new();

        // Start job and wait for completion
        let pod_run = orchestrator.start_blocking(namespace_lookup, &pod_job)?;
        sleep(Duration::from_secs(1));
        let pod_result = orchestrator.get_result_blocking(&pod_run)?;

        assert_eq!(
            pod_result.status,
            Status::Failed(127),
            "Should be in failed state"
        );

        // Clean up the pod
        orchestrator.delete_blocking(&pod_run)?;

        assert!(
            !orchestrator.list_blocking()?.contains(&pod_run),
            "Unexpected container remains."
        );

        Ok(())
    })
}

#[test]
fn test_queued_status_container() -> Result<()> {
    execute_wrapper(|namespace_lookup, orchestrator| {
        let mut pod_job = pod_job_style(namespace_lookup)?;
        let mut pod = pod_job.pod.deref().clone();

        pod.image = "alpine:3.14".to_owned();
        pod.command = "python file_does_not_exist.py".to_owned();
        pod.input_spec = HashMap::new();
        pod_job.pod = pod.into();
        pod_job.input_packet = HashMap::new();

        let runtime = Runtime::new()?;

        let image_options = Some(CreateImageOptions {
            from_image: pod_job.pod.image.clone(),
            ..Default::default()
        });
        runtime.block_on(
            orchestrator
                .api
                .create_image(image_options, None, None)
                .collect::<Vec<_>>(),
        );

        // Start job and wait for completion
        let (container_name, options, config) =
            LocalDockerOrchestrator::prepare_container_start_inputs(
                namespace_lookup,
                &pod_job,
                pod_job.pod.image.clone(),
            )?;

        runtime.block_on(orchestrator.api.create_container(options, config))?;

        // List all containers and check if the queued_container is in the list
        let pod_runs = orchestrator
            .list_blocking()?
            .into_iter()
            .filter(|pod_run| pod_run.assigned_name == container_name)
            .collect::<Vec<_>>();

        assert!(
            pod_runs.len() == 1,
            "List didn't return just the queued pod."
        );

        let pod_run = pod_runs.first().unwrap();

        // Check that the status is queued
        assert!(
            orchestrator.get_info_blocking(pod_run)?.status == Status::Queued,
            "Status is not queued"
        );

        // Clean up container
        orchestrator.delete_blocking(pod_run)?;
        assert!(
            !orchestrator.list_blocking()?.contains(pod_run),
            "Unexpected container remains."
        );

        Ok(())
    })
}
