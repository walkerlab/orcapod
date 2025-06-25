#![expect(
    missing_docs,
    clippy::panic_in_result_fn,
    clippy::panic,
    clippy::expect_used,
    reason = "OK in tests."
)]

pub mod fixture;
use fixture::{NAMESPACE_LOOKUP_READ_ONLY, pull_image};
use orcapod::uniffi::{
    error::Result,
    model::{Annotation, OrcaPath, Pod, PodJob, PodResult},
    orchestrator::{
        agent::{Agent, AgentClient},
        docker::LocalDockerOrchestrator,
    },
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{self, task::JoinSet, time::sleep as async_sleep};
use zenoh;

#[test]
fn simple() -> Result<()> {
    let (group, host) = ("test", "alpha");
    let _client = AgentClient::new(group.to_owned(), host.to_owned())?;
    let _agent = Agent::new(
        group.to_owned(),
        host.to_owned(),
        Arc::new(LocalDockerOrchestrator::new()?),
    )?;

    Ok(())
}

#[expect(clippy::excessive_nesting, reason = "Nesting is manageable")]
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn parallel_four_cores() -> Result<()> {
    // config
    let image_reference = "ghcr.io/colinianking/stress-ng:e2f96874f951a72c1c83ff49098661f0e013ac40";
    pull_image(image_reference)?;
    let margin_millis = 2000;
    let run_duration_secs = 5;
    let service_readiness_delay_secs = 1;
    let current_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Current time is earlier than start of epoch (1970-01-01 00:00:00).")
        .as_millis();
    println!("current_timestamp: {current_timestamp}");
    let (group, host) = ("test", "alpha");
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
        async move { inner_agent.start(&NAMESPACE_LOOKUP_READ_ONLY).await }
    });
    services.spawn(async move {
        let session = zenoh::open(zenoh::Config::default())
            .await
            .expect("Unable to create a zenoh session.");
        let subscriber = session
            .declare_subscriber(&format!("group/{group}/success/pod_job/**"))
            .await
            .expect("Unable to create subscriber.");
        let mut counter = 0;
        while let Ok(sample) = subscriber.recv_async().await {
            counter += 1;
            let pod_result = serde_json::from_slice::<PodResult>(&sample.payload().to_bytes())?;
            assert!(
                u128::from(pod_result.created * 1000)
                    <= current_timestamp + margin_millis + u128::from(service_readiness_delay_secs),
                "Started pod run too late."
            );
            assert!(
                u128::from(pod_result.terminated * 1000)
                    <= current_timestamp
                        + 2 * margin_millis
                        + run_duration_secs * 1000
                        + u128::from(service_readiness_delay_secs),
                "Took too long to finish pod run."
            );
            if counter == 4 {
                break;
            }
        }
        Ok(())
    });
    services.spawn(async {
        async_sleep(Duration::from_secs(60)).await;
        panic!("Test took too long. Killing...");
    });
    async_sleep(Duration::from_secs(service_readiness_delay_secs)).await;
    // submit requests
    let pod_jobs = (1..5)
        .map(|i| {
            Ok(Arc::new(PodJob::new(
                Some(Annotation {
                    name: "simple".to_owned(),
                    description: "This is an example pod job.".to_owned(),
                    version: format!("0.{i}.0"),
                }),
                Pod::new(
                    Some(Annotation {
                        name: "simple".to_owned(),
                        description: "This is an example pod.".to_owned(),
                        version: format!("{i}.0.0"),
                    }),
                    image_reference.into(),
                    format!("stress-ng --cpu 1 --cpu-load 100 --timeout {run_duration_secs} --metrics-brief"),
                    HashMap::new(),
                    PathBuf::from("/tmp/output"),
                    HashMap::new(),
                    "https://github.com/user/simple".to_owned(),
                    0.1,          // 100 millicores as frac cores
                    10_u64 << 20, // 10 MiB in bytes
                    None,
                )?
                .into(),
                HashMap::new(),
                OrcaPath {
                    namespace: "default".to_owned(),
                    path: PathBuf::from("."),
                },
                1.0,          // 1000 millicores as frac cores
                10_u64 << 20, // 2GiB in bytes, KiB=<<10, MiB=<<20, GiB=<<30
                None,
                &NAMESPACE_LOOKUP_READ_ONLY,
            )?))
        })
        .collect::<Result<Vec<_>>>()?;
    client.submit_pod_jobs(pod_jobs).await;

    services
        .join_next()
        .await
        .expect("Services unexpectedly empty")?
}
