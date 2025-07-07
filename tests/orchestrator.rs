#![expect(missing_docs, clippy::panic_in_result_fn, reason = "OK in tests.")]

pub mod fixture;
use fixture::{
    NAMESPACE_LOOKUP_READ_ONLY, TestContainerImage, TestDirs, container_image_style,
    pod_job_custom, pod_job_style, pod_jobs_stresser, pod_result_style, str_to_vec,
};
use futures_util::future::join_all;
use orcapod::uniffi::{
    error::{OrcaError, Result},
    model::{PathSet, URI},
    orchestrator::{ImageKind, Orchestrator as _, PodRun, Status, docker::LocalDockerOrchestrator},
};
use std::{collections::HashMap, path::PathBuf};

fn basic_test<T>(start: T) -> Result<()>
where
    T: Fn(
        &HashMap<String, PathBuf>,
        &LocalDockerOrchestrator,
    ) -> Result<(
        PodRun,
        Vec<String>,
        HashMap<String, PathSet>,
        Option<TestContainerImage>,
    )>,
{
    let test_dirs = TestDirs::new(&HashMap::from([(
        "default".to_owned(),
        Some("./tests/extra/data/"),
    )]))?;
    let namespace_lookup = test_dirs.namespace_lookup();
    let orchestrator = LocalDockerOrchestrator::new()?;
    let (pod_run, expected_command, expected_output_packet, _container_image) =
        start(&namespace_lookup, &orchestrator)?;
    assert_eq!(
        orchestrator.get_info_blocking(&pod_run)?.status,
        Status::Running,
        "Unexpected state."
    );
    assert_eq!(
        orchestrator
            .list_blocking()?
            .iter()
            .filter(|container| container.pod_job == pod_run.pod_job)
            .map(|run| Ok(orchestrator.get_info_blocking(run)?.command))
            .collect::<Result<Vec<_>>>()?,
        vec![expected_command.clone()],
        "Unexpected list."
    );
    // await result
    let pod_result_1 = orchestrator.get_result_blocking(&namespace_lookup, &pod_run)?;
    assert_eq!(
        orchestrator.get_info_blocking(&pod_run)?.status,
        Status::Completed,
        "Unexpected state."
    );
    assert_eq!(
        pod_result_1.output_packet, expected_output_packet,
        "Unexpected output packet.",
    );
    assert_eq!(
        orchestrator
            .list_blocking()?
            .iter()
            .filter(|container| container.pod_job == pod_run.pod_job)
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
    let pod_result_2 = orchestrator.get_result_blocking(&namespace_lookup, &pod_run)?;
    assert_eq!(pod_result_1, pod_result_2, "Pod results don't match.");
    // test delete
    orchestrator.delete_blocking(&pod_run)?;
    assert!(
        !orchestrator
            .list_blocking()?
            .iter()
            .any(|container| container.pod_job == pod_run.pod_job),
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
        let container_image_kind = ImageKind::Tarball {
            image_uri: URI {
                namespace: "default".to_owned(),
                path: PathBuf::from(container_image_relative_location),
            },
        };
        let container_image = container_image_style(
            namespace_lookup["default"].join(container_image_relative_location),
        )?;
        let mut pod_job = pod_job_style(namespace_lookup)?;
        pod_job.env_vars = Some(HashMap::from([("DELAY".to_owned(), "5".to_owned())]));
        let expected_pod_result = pod_result_style(&NAMESPACE_LOOKUP_READ_ONLY)?;
        Ok((
            orchestrator.start_with_altimage_blocking(
                namespace_lookup,
                &pod_job,
                &container_image_kind,
            )?,
            pod_job.pod.command.clone(),
            expected_pod_result.output_packet,
            Some(container_image),
        ))
    })
}

#[test]
fn remote_container_image_basic() -> Result<()> {
    basic_test(|namespace_lookup, orchestrator| {
        let pod_job = pod_job_custom("alpine:3.14", &str_to_vec("sleep 5"), namespace_lookup)?;
        Ok((
            orchestrator.start_blocking(namespace_lookup, &pod_job)?,
            pod_job.pod.command.clone(),
            HashMap::new(),
            None,
        ))
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn remote_container_image_failed() -> Result<()> {
    let orch = LocalDockerOrchestrator::new()?;
    let pod_job = pod_job_custom(
        "alpine:3.14",
        &str_to_vec("sleep crash"),
        &NAMESPACE_LOOKUP_READ_ONLY,
    )?;
    let pod_run = orch.start(&NAMESPACE_LOOKUP_READ_ONLY, &pod_job).await?;
    let pod_result = orch
        .get_result(&NAMESPACE_LOOKUP_READ_ONLY, &pod_run)
        .await?;
    orch.delete(&pod_run).await?;

    assert!(
        matches!(pod_result.status, Status::Failed { exit_code: 1 }),
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
            let pod_run = orch.start(&NAMESPACE_LOOKUP_READ_ONLY, pod_job).await?;
            let pod_result = orch
                .get_result(&NAMESPACE_LOOKUP_READ_ONLY, &pod_run)
                .await?;
            orch.delete(&pod_run).await?;
            Ok::<_, OrcaError>(pod_result)
        }),
    )
    .await;

    let statuses = results
        .into_iter()
        .map(|result| Ok(result?.status))
        .filter(|status| !matches!(status, Ok(Status::Completed)))
        .collect::<Result<Vec<_>>>()?;

    println!("statuses: {statuses:?}");
    assert!(
        statuses.is_empty(),
        "Some pod results returned in a status other than `Completed`."
    );
    Ok(())
}
