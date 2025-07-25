#![expect(
    missing_docs,
    clippy::panic,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "OK in tests."
)]

pub mod fixture;
use fixture::{TestDirs, pipeline_job_adder};
use names::{Generator, Name};
use orcapod::uniffi::{
    error::Result,
    model::PipelineJob,
    orchestrator::{
        agent::{Agent, AgentClient},
        docker::LocalDockerOrchestrator,
    },
    pipeline::PipelineStatus,
    store::filestore::LocalFileStore,
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{task::JoinSet, time::sleep as async_sleep};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn adder_success() -> Result<()> {
    verify_pipeline(
        |data_dir_filepath, namespace_lookup| {
            pipeline_job_adder(5, &[], data_dir_filepath, namespace_lookup)
        },
        8,
        PipelineStatus::Completed,
        15,
    )
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn adder_failure() -> Result<()> {
    verify_pipeline(
        |data_dir_filepath, namespace_lookup| {
            pipeline_job_adder(5, &["add_d"], data_dir_filepath, namespace_lookup)
        },
        8,
        PipelineStatus::Failed,
        10,
    )
    .await
}

async fn verify_pipeline(
    make_pipeline_job: fn(&Path, &HashMap<String, PathBuf>) -> Result<PipelineJob>,
    seconds_until_node_completed: u64,
    expected_pipeline_status: PipelineStatus,
    expected_runtime: u64,
) -> Result<()> {
    let test_dirs = TestDirs::new(&HashMap::from([("default".to_owned(), None::<String>)]))?;
    let data_dir_filepath = test_dirs.namespace_lookup()["default"].join("data");
    let store = LocalFileStore::new(test_dirs.namespace_lookup()["default"].join("store"));
    // config
    let margin_millis = 6;
    let (group, host) = (
        Generator::with_naming(Name::Plain)
            .next()
            .expect("Generated names overflowed."),
        "host".to_owned(),
    );
    // api
    let client = AgentClient::new(group.clone(), host.clone())?;
    let agent = Agent::new(group, host, Arc::new(LocalDockerOrchestrator::new()?))?;
    // background services
    let mut services = JoinSet::new();
    services.spawn({
        let inner_client = client.clone();
        async move { inner_client.watch("**".to_owned()).await }
    });
    services.spawn({
        let inner_agent = agent.clone();
        let inner_store = store.clone();
        let namespace_lookup = test_dirs.namespace_lookup();
        async move {
            inner_agent
                .start(&namespace_lookup, Some(inner_store.into()))
                .await
        }
    });
    // setup pipeline
    let pipeline_job =
        make_pipeline_job(data_dir_filepath.as_path(), &test_dirs.namespace_lookup())?;
    assert!(
        pipeline_job.pipeline.make_dot(true)?.contains("bold"),
        "Pipeline DAG does not include style."
    );
    // submit request
    services.spawn(async move {
        let pipeline_run = client.start_pipeline_job(pipeline_job.into()).await?;
        assert_eq!(
            pipeline_run.status(),
            PipelineStatus::Running,
            "Pipeline not running."
        );
        async_sleep(Duration::from_secs(seconds_until_node_completed)).await; // wait until some progress
        assert!(
            pipeline_run.summarize_dot()?.contains("green"),
            "No pipeline node has successfully completed."
        );

        let pipeline_run_pointer = Arc::new(pipeline_run);
        let pipeline_result = client
            .get_pipeline_result(Arc::clone(&pipeline_run_pointer))
            .await?;
        // give pipeline run a chance to update its status
        // give watch console stream a chance to catch up
        async_sleep(Duration::from_secs(1)).await;
        let pipeline_result_again = client
            .get_pipeline_result(Arc::clone(&pipeline_run_pointer))
            .await?;

        assert_eq!(
            pipeline_result, pipeline_result_again,
            "Pipeline results inconsistent."
        );
        assert_eq!(
            pipeline_result.status, expected_pipeline_status,
            "Pipeline status unexpected."
        );
        assert!(
            pipeline_run_pointer
                .summarize_dot()?
                .contains("Pipeline not active"),
            "Pipeline summary report incorrect."
        );
        let actual_runtime = pipeline_result.terminated - pipeline_result.created;
        let expected_runtime_with_margin = expected_runtime + margin_millis;
        assert!(
            actual_runtime <= expected_runtime_with_margin,
            "Pipeline took too long (actual={actual_runtime}, expected={expected_runtime_with_margin})."
        );
        Ok(())
    });
    services.spawn(async {
        async_sleep(Duration::from_secs(60)).await;
        panic!("Test took too long. Killing...");
    });

    services
        .join_next()
        .await
        .expect("Services unexpectedly empty")?
}
