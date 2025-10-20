#![expect(
    missing_docs,
    clippy::panic_in_result_fn,
    clippy::panic,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "OK in tests."
)]

pub mod fixture;
use fixture::{TestDirs, container_image_style, pod_job_style};
use futures_util::future::join_all;
use orcapod::uniffi::{
    error::{OrcaError, Result},
    model::packet::URI,
    orchestrator::{ImageKind, Orchestrator, PodRun, PodStatus, docker::LocalDockerOrchestrator},
};
use std::{collections::HashMap, path::PathBuf};

use crate::fixture::{
    NAMESPACE_LOOKUP_READ_ONLY, pod_custom, pod_job_custom, pod_jobs_stresser, str_to_vec,
};
fn execute_wrapper<T>(test_fn: T) -> Result<()>
where
    T: Fn(&LocalDockerOrchestrator, &HashMap<String, PathBuf>) -> Result<()>,
{
    let test_dirs = TestDirs::new(&HashMap::from([(
        "default".to_owned(),
        Some("./tests/extra/data/"),
    )]))?;
    let namespace_lookup = test_dirs.namespace_lookup();
    let orchestrator = LocalDockerOrchestrator::new()?;

    test_fn(&orchestrator, &namespace_lookup)
}

fn basic_test(
    pod_run: &PodRun,
    expected_command: &[String],
    orchestrator: &impl Orchestrator,
    namespace_lookup: &HashMap<String, PathBuf>,
) -> Result<()> {
    assert_eq!(
        orchestrator.get_info_blocking(pod_run)?.status,
        PodStatus::Running,
        "Unexpected state."
    );
    assert_eq!(
        orchestrator
            .list_blocking()?
            .into_iter()
            .filter(|pod_run_from_list| pod_run_from_list == pod_run)
            .map(|run| Ok(orchestrator.get_info_blocking(&run)?.command))
            .collect::<Result<Vec<_>>>()?,
        vec![expected_command],
        "Unexpected list."
    );
    // await result
    let pod_result_1 = orchestrator.get_result_blocking(pod_run, namespace_lookup)?;
    assert_eq!(
        orchestrator.get_info_blocking(pod_run)?.status,
        PodStatus::Completed,
        "Unexpected state."
    );

    assert_eq!(
        orchestrator
            .list_blocking()?
            .into_iter()
            .filter(|pod_run_from_list| pod_run_from_list == pod_run)
            .map(|run| Ok(orchestrator.get_info_blocking(&run)?.command))
            .collect::<Result<Vec<_>>>()?,
        vec![expected_command],
        "Unexpected list."
    );
    assert_eq!(
        pod_result_1.assigned_name, pod_run.assigned_name,
        "Unexpected name."
    );
    // try generating result again
    let pod_result_2 = orchestrator.get_result_blocking(pod_run, namespace_lookup)?;
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
    execute_wrapper(|orchestrator, namespace_lookup| {
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
                &pod_job,
                &container_image_kind,
                namespace_lookup,
            )?,
            &pod_job.pod.command,
            orchestrator,
            namespace_lookup,
        )?;
        Ok(())
    })
}

#[test]
fn remote_container_image_basic() -> Result<()> {
    execute_wrapper(|orchestrator, namespace_lookup| {
        let pod_job = pod_job_custom(
            pod_custom("alpine:3.14", str_to_vec("sleep 5"), HashMap::new())?,
            HashMap::new(),
            namespace_lookup,
        )?;

        basic_test(
            &orchestrator.start_blocking(&pod_job, namespace_lookup)?,
            &pod_job.pod.command,
            orchestrator,
            namespace_lookup,
        )?;
        Ok(())
    })
}

#[test]
/// Expect pod to fail due to bad command, where the expected behavior should auto delete the container and return an error
fn fail_at_start() -> Result<()> {
    execute_wrapper(|orchestrator, namespace_lookup| {
        let pod_job = pod_job_custom(
            pod_custom(
                "alpine:3.14",
                vec!["invalid_command".into()],
                HashMap::new(),
            )?,
            HashMap::new(),
            namespace_lookup,
        )?;

        let container_name = match orchestrator.start_blocking(&pod_job, namespace_lookup) {
            Ok(_) => panic!("Pod was launched successfully when it should have failed."),
            Err(err) => {
                assert!(err.is_failed_to_start_pod());
                err.get_container_name()
                    .expect("Failed to get container name from error.")
            }
        };

        // Make sure the pod has been deleted after failing to start
        let pod_runs = orchestrator
            .list_blocking()?
            .into_iter()
            .filter(|pod_run| pod_run.assigned_name == *container_name)
            .collect::<Vec<_>>();

        assert!(pod_runs.len() == 1, "List didn't return just the fail pod.");

        let pod_run = pod_runs.first().unwrap();

        // Get the pod result and make sure it is in failed state
        let pod_result = orchestrator.get_result_blocking(pod_run, namespace_lookup)?;

        assert_eq!(
            pod_result.status,
            PodStatus::Failed(127),
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
    execute_wrapper(|orchestrator, namespace_lookup| {
        let pod_job = pod_job_custom(
            pod_custom(
                "alpine:3.14",
                vec![
                    "bin/sh".into(),
                    "-c".into(),
                    r#"echo "hi" && bad_command"#.into(),
                ],
                HashMap::new(),
            )?,
            HashMap::new(),
            namespace_lookup,
        )?;

        // Start job and wait for completion
        let pod_run = orchestrator.start_blocking(&pod_job, namespace_lookup)?;
        let pod_result = orchestrator.get_result_blocking(&pod_run, namespace_lookup)?;

        assert_eq!(
            pod_result.status,
            PodStatus::Failed(127),
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

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn remote_container_image_failed() -> Result<()> {
    let pod_job = pod_job_custom(
        pod_custom("alpine:3.14", str_to_vec("sleep crash"), HashMap::new())?,
        HashMap::new(),
        &NAMESPACE_LOOKUP_READ_ONLY,
    )?;

    let orch = LocalDockerOrchestrator::new()?;
    let pod_run = orch.start(&pod_job, &NAMESPACE_LOOKUP_READ_ONLY).await?;
    let pod_result = orch
        .get_result(&pod_run, &NAMESPACE_LOOKUP_READ_ONLY)
        .await?;
    orch.delete(&pod_run).await?;

    assert!(
        matches!(pod_result.status, PodStatus::Failed(1)),
        "Expected to fail but did not."
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[expect(clippy::use_debug, reason = "Useful in debugging from CI.")]
async fn verify_pod_result_not_running() -> Result<()> {
    let results = join_all(
        pod_jobs_stresser(
            "ghcr.io/colinianking/stress-ng:e2f96874f951a72c1c83ff49098661f0e013ac40",
            5,
            16,
            0,
        )?
        .iter()
        .map(|pod_job| async move {
            let orch = LocalDockerOrchestrator::new()?;
            let pod_run = orch.start(pod_job, &NAMESPACE_LOOKUP_READ_ONLY).await?;
            let pod_result = orch
                .get_result(&pod_run, &NAMESPACE_LOOKUP_READ_ONLY)
                .await?;
            orch.delete(&pod_run).await?;
            Ok::<_, OrcaError>(pod_result)
        }),
    )
    .await;

    let statuses = results
        .into_iter()
        .map(|result| Ok(result?.status))
        .filter(|status| !matches!(status, Ok(PodStatus::Completed)))
        .collect::<Result<Vec<_>>>()?;

    println!("statuses: {statuses:?}");
    assert!(
        statuses.is_empty(),
        "Some pod results returned in a status other than `Completed`."
    );
    Ok(())
}

#[test]
fn logs() -> Result<()> {
    execute_wrapper(|orchestrator, namespace_lookup| {
        let pod_job = pod_job_custom(
            pod_custom(
                "alpine:3.14",
                vec!["bin/sh".into(), "-c".into(), "echo \"hi\"".into()],
                HashMap::new(),
            )?,
            HashMap::new(),
            namespace_lookup,
        )?;

        let pod_run = orchestrator.start_blocking(&pod_job, namespace_lookup)?;
        let pod_result = orchestrator.get_result_blocking(&pod_run, namespace_lookup)?;

        assert_eq!(
            pod_result.status,
            PodStatus::Completed,
            "Pod status is not completed"
        );

        assert_eq!(orchestrator.get_logs_blocking(&pod_run)?, "hi\n");
        assert_eq!(
            orchestrator
                .get_result_blocking(&pod_run, namespace_lookup)?
                .logs,
            "hi\n"
        );

        orchestrator.delete_blocking(&pod_run)?;

        assert!(
            !orchestrator.list_blocking()?.contains(&pod_run),
            "Unexpected container remains."
        );

        Ok(())
    })
}
