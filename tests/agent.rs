#![expect(
    missing_docs,
    clippy::panic_in_result_fn,
    clippy::panic,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "OK in tests."
)]

pub mod fixture;
use fixture::{NAMESPACE_LOOKUP_READ_ONLY, TestDirs, pod_jobs_stresser, pull_image};
use orcapod::uniffi::{
    error::Result,
    model::pod::PodResult,
    orchestrator::{
        agent::{Agent, AgentClient},
        docker::LocalDockerOrchestrator,
    },
    store::{ModelID, Store as _, filestore::LocalFileStore},
};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{self, task::JoinSet, time::sleep as async_sleep};
use zenoh;

#[test]
fn simple() -> Result<()> {
    let (group, host) = ("agent_simple", "host");
    let _client = AgentClient::new(group.to_owned(), host.to_owned())?;
    let _agent = Agent::new(
        group.to_owned(),
        host.to_owned(),
        Arc::new(LocalDockerOrchestrator::new()?),
    )?;

    Ok(())
}

#[expect(clippy::excessive_nesting, reason = "Nesting is manageable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn parallel_four_cores() -> Result<()> {
    let test_dirs = TestDirs::new(&HashMap::from([("default".to_owned(), None::<String>)]))?;
    let store = LocalFileStore::new(test_dirs.0["default"].path().into());
    // config
    let image_reference = "ghcr.io/colinianking/stress-ng:e2f96874f951a72c1c83ff49098661f0e013ac40";
    pull_image(image_reference)?;
    let margin_millis = 2000;
    let run_duration_secs = 5;
    let current_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Current time is earlier than start of epoch (1970-01-01 00:00:00).")
        .as_millis();
    println!("current_timestamp: {current_timestamp}");
    let (group, host) = ("agent_parallel-four-cores", "host");
    // api
    let client = AgentClient::new(group.to_owned(), host.to_owned())?;
    let agent = Agent::new(
        group.to_owned(),
        host.to_owned(),
        Arc::new(LocalDockerOrchestrator::new()?),
    )?;
    // background services
    let mut services = JoinSet::new();
    services.spawn({
        let inner_client = client.clone();
        async move { inner_client.watch("**".to_owned()).await }
    });
    services.spawn({
        let inner_agent = agent.clone();
        let inner_store = store.clone();
        async move {
            inner_agent
                .start(&NAMESPACE_LOOKUP_READ_ONLY, Some(inner_store.into()))
                .await
        }
    });
    services.spawn(async move {
        let session = zenoh::open(zenoh::Config::default())
            .await
            .expect("Unable to create a zenoh session.");
        let subscriber = session
            .declare_subscriber(&format!("group/{group}/*/pod_job/**"))
            .await
            .expect("Unable to create subscriber.");
        let mut success_counter = 0;
        let mut failure_counter = 0;
        while let Ok(sample) = subscriber.recv_async().await {
            let topic_kind = sample.key_expr().as_str().split('/').collect::<Vec<_>>()[2];
            if ["success", "failure"].contains(&topic_kind) {
                let pod_result = serde_json::from_slice::<PodResult>(&sample.payload().to_bytes())?;
                assert!(
                    u128::from(pod_result.created * 1000) <= current_timestamp + margin_millis,
                    "Started pod run too late."
                );
                assert!(
                    u128::from(pod_result.terminated * 1000)
                        <= current_timestamp
                            + 2 * margin_millis
                            + u128::from(run_duration_secs * 1000),
                    "Took too long to finish pod run."
                );
                async_sleep(Duration::from_secs(1)).await; // give agent a chance to save pod result first
                assert_eq!(
                    store.load_pod_result(&ModelID::Hash(pod_result.hash.clone()))?,
                    pod_result,
                    "Stored pod result does not match."
                );
                if topic_kind == "success" {
                    success_counter += 1;
                } else {
                    failure_counter += 1;
                }
                if success_counter == 3 && failure_counter == 1 {
                    break;
                }
            }
        }
        Ok(())
    });
    services.spawn(async {
        async_sleep(Duration::from_secs(60)).await;
        panic!("Test took too long. Killing...");
    });
    // submit requests
    client
        .start_pod_jobs(pod_jobs_stresser(image_reference, run_duration_secs, 3, 1)?)
        .await;

    services
        .join_next()
        .await
        .expect("Services unexpectedly empty")?
}
