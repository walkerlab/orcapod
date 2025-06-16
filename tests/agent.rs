#![expect(
    missing_docs,
    clippy::too_many_lines,
    clippy::expect_used,
    clippy::excessive_nesting,
    reason = "OK in tests."
)]

// some useful debug commands:
//
// $ docker rm -f $(docker ps -aq) || echo empty && watch -n 1 "bash -c 'docker ps -a | echo \$((\$(wc -l)-1)) && docker ps -a'"
// $ top

pub mod fixture;
use fixture::NAMESPACE_LOOKUP_READ_ONLY;
use futures::{StreamExt as _, TryStreamExt as _};
use futures_util::{
    future::{join_all, try_join_all},
    stream,
};
use orcapod::{
    core::util::ASYNC_RUNTIME,
    uniffi::{
        error::{OrcaError, Result, selector},
        model::{Annotation, OrcaPath, Pod, PodJob, PodResult},
        orchestrator::{
            PodRun, Status,
            agent::{Agent, AgentClient},
            docker::LocalDockerOrchestrator,
        },
    },
};
use snafu::ResultExt as _;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    thread::sleep,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    join,
    sync::{Mutex, MutexGuard, broadcast, watch},
    task::spawn,
};

async fn publish(client: Arc<AgentClient>) -> Result<()> {
    Ok(client
        .session
        .put("request", "something")
        .await
        .context(selector::AgentFailure {})?)
}

#[test]
fn docker_listener() -> Result<()> {
    let agent = Agent::new(
        "test".to_owned(),
        "alpha".to_owned(),
        LocalDockerOrchestrator::new()?.into(),
        true,
    )?;
    let client = Arc::clone(&agent.client);

    let pod_jobs = (1..11)
        .map(|i| {
            PodJob::new(
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
                    // "alpine:3.14".to_owned(),
                    "ghcr.io/colinianking/stress-ng:e2f96874f951a72c1c83ff49098661f0e013ac40"
                        .to_owned(),
                    // format!("sleep {}", i * 5),
                    // "sleep 5".to_owned(),
                    "stress-ng --cpu 1 --cpu-load 100 --timeout 5 --metrics-brief".to_owned(),
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
            )
        })
        .collect::<Result<Vec<_>>>()?;

    let service_handle = ASYNC_RUNTIME.spawn(async move {
        let mut counter = 0;
        println!("Started");
        let subscriber = agent
            .client
            .session
            .declare_subscriber("request")
            // .callback(|sample| {
            //     counter += 1;
            //     let _pod_results = concurrent(pod_jobs.clone(), &agent).await?;
            //     if counter == 2 {
            //         println!("Received 2 messages, exiting listener.");
            //         // break;
            //     }
            // })
            .await
            .context(selector::AgentFailure {})?;
        while let Ok(sample) = subscriber.recv_async().await {
            counter += 1;
            println!("Received");
            // if let Ok(pod_job) = serde_json::from_slice::<PodJob>(&sample.payload().to_bytes()) {
            // let pod_jobs = (1..11)
            //     .map(|i| {
            //         PodJob::new(
            //             Some(Annotation {
            //                 name: "simple".to_owned(),
            //                 description: "This is an example pod job.".to_owned(),
            //                 version: format!("0.{i}.0"),
            //             }),
            //             Pod::new(
            //                 Some(Annotation {
            //                     name: "simple".to_owned(),
            //                     description: "This is an example pod.".to_owned(),
            //                     version: format!("{i}.0.0"),
            //                 }),
            //                 // "alpine:3.14".to_owned(),
            //                 "ghcr.io/colinianking/stress-ng:e2f96874f951a72c1c83ff49098661f0e013ac40"
            //                     .to_owned(),
            //                 // format!("sleep {}", i * 5),
            //                 // "sleep 5".to_owned(),
            //                 "stress-ng --cpu 1 --cpu-load 100 --timeout 5 --metrics-brief".to_owned(),
            //                 HashMap::new(),
            //                 PathBuf::from("/tmp/output"),
            //                 HashMap::new(),
            //                 "https://github.com/user/simple".to_owned(),
            //                 0.1,          // 100 millicores as frac cores
            //                 10_u64 << 20, // 10 MiB in bytes
            //                 None,
            //             )?
            //             .into(),
            //             HashMap::new(),
            //             OrcaPath {
            //                 namespace: "default".to_owned(),
            //                 path: PathBuf::from("."),
            //             },
            //             1.0,          // 1000 millicores as frac cores
            //             10_u64 << 20, // 2GiB in bytes, KiB=<<10, MiB=<<20, GiB=<<30
            //             None,
            //             &NAMESPACE_LOOKUP_READ_ONLY,
            //         )
            //     })
            //     .collect::<Result<Vec<_>>>()?;
            let _pod_results = concurrent(pod_jobs.clone(), &agent).await?;
            //     // if let Some(history) = &history_option {
            //     //     history.lock().unwrap().insert(
            //     //         DateTime::parse_from_rfc3339(&capture["timestamp"])?.into(),
            //     //         Event {
            //     //             group: capture["group"].to_string(),
            //     //             host: capture["host"].to_string(),
            //     //             subgroup: capture["pod_job_hash"].to_string(),
            //     //             payload: EventPayload::Request(pod_job),
            //     //         },
            //     //     );
            //     //     client.log(&format!("History: {history:#?}")).await?;
            //     // }
            //     // println!("Made it 1");
            //     // let pod_jobs = serde_json::from_slice::<Vec<PodJob>>(
            //     //     &client
            //     //         .session
            //     //         .get(format!("group/{}/request/pod_job/1123", client.group))
            //     //         .await
            //     //         .context(selector::AgentFailure {})?
            //     //         .iter()
            //     //         .next()
            //     //         .unwrap()
            //     //         .into_result()?
            //     //         .payload()
            //     //         .to_bytes(),
            //     // )?;
            //     // println!("Made it 3");

            //     // for pj in pod_jobs {
            //     //     let message = format!("pod_job command: {}", pj.pod.command);
            //     //     println!("{}", &message);
            //     //     client.log(&message).await?;
            //     // }
            // }
            if counter == 2 {
                println!("Received 2 messages, exiting listener.");
                break;
            }
        }
        Ok::<_, OrcaError>(())
    });

    sleep(Duration::from_secs(5)); // wait until start service ready

    println!("Sending 1st request.");
    ASYNC_RUNTIME.block_on(publish(Arc::clone(&client)))?;

    sleep(Duration::from_secs(2)); // send in between and see if processes async or awaits previous request

    println!("Sending 2nd request.");
    ASYNC_RUNTIME.block_on(publish(Arc::clone(&client)))?;

    ASYNC_RUNTIME.block_on(async { service_handle.await? })?;

    Ok(())
}

fn procedural(pod_jobs: &[PodJob], agent: &Agent) -> Result<Vec<PodResult>> {
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs();

    let pod_results = pod_jobs
        .iter()
        .map(|pod_job| {
            ASYNC_RUNTIME.block_on(async {
                let pod_run = agent
                    .orchestrator
                    .start(&NAMESPACE_LOOKUP_READ_ONLY, pod_job)
                    .await?;
                agent.orchestrator.get_result(&pod_run).await
            })
        })
        .collect::<Result<Vec<_>>>();

    println!(
        "started: {started}, duration: {}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs()
            - started
    );

    pod_results
}

async fn concurrent(pod_jobs: Vec<PodJob>, agent: &Agent) -> Result<Vec<PodResult>> {
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs();

    let pod_job_handles = pod_jobs.into_iter().map(|pod_job| async move {
        let pod_run = agent
            .orchestrator
            .start(&NAMESPACE_LOOKUP_READ_ONLY, &pod_job)
            .await?;
        agent.orchestrator.get_result(&pod_run).await
    });
    let pod_results = stream::iter(pod_job_handles)
        // .buffer_unordered(1)  // procedural
        // .buffer_unordered(20) // concurrent launching 20 max
        .buffer_unordered(usize::MAX) // concurrent launching all
        .try_collect::<Vec<_>>()
        .await;

    println!(
        "started: {started}, duration: {}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs()
            - started
    );

    pod_results
}

// based on loops which is wasteful, sleep helps but extends...
async fn concurrent_with_limit(
    pod_jobs: Vec<PodJob>,
    agent: &Agent,
    memory_limit: u64,
    cpu_limit: f32,
) -> Result<Vec<PodResult>> {
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs();

    let check = Arc::new(Mutex::new(()));
    let pod_job_handles = pod_jobs.into_iter().map(|pod_job| {
        let resource_check = Arc::clone(&check);
        async move {
            let pod_result: Result<PodResult>;
            loop {
                let resource_check_lock = resource_check.lock().await;
                let run_infos = try_join_all(
                    agent
                        .orchestrator
                        .list()
                        .await?
                        .iter()
                        .map(|pod_run| async { agent.orchestrator.get_info(pod_run).await }),
                )
                .await?;
                let (reserved_memory, reserved_cpu) = run_infos
                    .iter()
                    .filter(|&run_info| run_info.status == Status::Running)
                    .fold((0_u64, 0_f32), |(total_memory, total_cpu), run_info| {
                        (
                            total_memory + run_info.memory_limit,
                            total_cpu + run_info.cpu_limit,
                        )
                    });
                if memory_limit >= reserved_memory + pod_job.memory_limit
                    && cpu_limit >= reserved_cpu + pod_job.cpu_limit
                {
                    let pod_run = agent
                        .orchestrator
                        .start(&NAMESPACE_LOOKUP_READ_ONLY, &pod_job)
                        .await?;
                    drop(resource_check_lock);
                    pod_result = agent.orchestrator.get_result(&pod_run).await;
                    break;
                }
                sleep(Duration::from_secs(1)); // performance better if polling rather than continuous
            }
            pod_result
        }
    });
    let pod_results = try_join_all(pod_job_handles).await; // parallel with no "no limit"

    println!(
        "started: {started}, duration: {}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs()
            - started
    );

    pod_results
}

#[expect(clippy::shadow_unrelated, clippy::unwrap_used, reason = "debug")]
async fn concurrent_with_limit_via_channel(
    pod_jobs: Vec<PodJob>,
    agent: &Agent,
    memory_limit: u64,
    cpu_limit: f32,
) -> Result<Vec<PodResult>> {
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs();

    let resource = Arc::new(Mutex::new((0_u64, 0_f32)));
    // let (root_tx, _) = broadcast::channel(10);
    let (root_tx, _) = watch::channel(());
    let pod_job_handles = pod_jobs.into_iter().map(|pod_job| {
        let resource_usage = Arc::clone(&resource);
        let tx = root_tx.clone();
        let mut rx = root_tx.subscribe();
        async move {
            let pod_run: PodRun;
            loop {
                let mut loaded_resource = resource_usage.lock().await;
                if memory_limit >= loaded_resource.0 + pod_job.memory_limit
                    && cpu_limit >= loaded_resource.1 + pod_job.cpu_limit
                {
                    (loaded_resource.0, loaded_resource.1) = (
                        loaded_resource.0 + pod_job.memory_limit,
                        loaded_resource.1 + pod_job.cpu_limit,
                    );
                    pod_run = agent
                        .orchestrator
                        .start(&NAMESPACE_LOOKUP_READ_ONLY, &pod_job)
                        .await?;
                    break;
                }
                drop(loaded_resource);
                rx.changed().await.unwrap();
                // rx.recv().await.unwrap();
            }

            let pod_result = agent.orchestrator.get_result(&pod_run).await;

            let mut unloaded_resource = resource_usage.lock().await;
            (unloaded_resource.0, unloaded_resource.1) = (
                unloaded_resource.0 - pod_job.memory_limit,
                unloaded_resource.1 - pod_job.cpu_limit,
            );
            drop(unloaded_resource);
            tx.send(()).unwrap();

            pod_result
        }
    });
    let pod_results = try_join_all(pod_job_handles).await; // parallel with no "no limit"

    println!(
        "started: {started}, duration: {}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs()
            - started
    );

    pod_results
}

// async fn parallel(pod_jobs: &[PodJob], agent: &Agent) -> Result<Vec<PodResult>> {
//     let pod_job_handles = pod_jobs
//         .iter()
//         .map(|pod_job| {
//             spawn(async {
//                 let pod_run = agent
//                     .orchestrator
//                     .start(&NAMESPACE_LOOKUP_READ_ONLY, pod_job)
//                     .await?;
//                 agent.orchestrator.get_result(&pod_run).await
//             })
//         })
//         .collect::<Vec<_>>();
//     try_join_all(pod_job_handles).await
// }

#[test]
fn docker_starter() -> Result<()> {
    let agent = Agent::new(
        "test".to_owned(),
        "alpha".to_owned(),
        LocalDockerOrchestrator::new()?.into(),
        true,
    )?;
    let agent_client = Arc::clone(&agent.client);

    let pod_jobs = (1..21)
        .map(|i| {
            PodJob::new(
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
                    // "alpine:3.14".to_owned(),
                    "ghcr.io/colinianking/stress-ng:e2f96874f951a72c1c83ff49098661f0e013ac40"
                        .to_owned(),
                    // format!("sleep {}", i * 5),
                    // "sleep 5".to_owned(),
                    "stress-ng --cpu 1 --cpu-load 100 --timeout 5 --metrics-brief".to_owned(),
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
                0.1,          // 1000 millicores as frac cores
                10_u64 << 20, // 2GiB in bytes, KiB=<<10, MiB=<<20, GiB=<<30
                None,
                &NAMESPACE_LOOKUP_READ_ONLY,
            )
        })
        .collect::<Result<Vec<_>>>()?;

    // procedurally, 5 * 20 = ~= 118s
    // let pod_results = procedural(&pod_jobs, &agent)?;

    // concurrent, 5 * 20 ~= 7s
    // let pod_results = ASYNC_RUNTIME.block_on(concurrent(pod_jobs, &agent))?;

    // concurrent with limits, 5 * 20 ~= 34s
    // let pod_results =
    //     ASYNC_RUNTIME.block_on(concurrent_with_limit(pod_jobs, &agent, 8_u64 << 30, 0.5))?;

    // concurrent with limits, 5 * 20 ~= 26s
    let pod_results = ASYNC_RUNTIME.block_on(concurrent_with_limit_via_channel(
        pod_jobs,
        &agent,
        8_u64 << 30,
        0.5,
    ))?;

    Ok(())
}

#[test]
fn start() -> Result<()> {
    let agent = Agent::new(
        "test".to_owned(),
        "alpha".to_owned(),
        LocalDockerOrchestrator::new()?.into(),
        true,
    )?;
    let agent_client = Arc::clone(&agent.client);

    let start_handle =
        ASYNC_RUNTIME.spawn(async move { agent.start(&NAMESPACE_LOOKUP_READ_ONLY, &None).await });

    sleep(Duration::from_secs(5));

    let submit_pod_job_handle = ASYNC_RUNTIME.spawn(async move {
        agent_client
            .submit_pod_jobs(
                (1..3)
                    .map(|i| {
                        Ok(PodJob::new(
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
                                "alpine:3.14".to_owned(),
                                format!("sleep {i}"),
                                HashMap::new(),
                                PathBuf::from("/tmp/output"),
                                HashMap::new(),
                                "https://github.com/user/simple".to_owned(),
                                0.1,          // 250 millicores as frac cores
                                10_u64 << 20, // 10 MiB in bytes
                                None,
                            )?
                            .into(),
                            HashMap::new(),
                            OrcaPath {
                                namespace: "default".to_owned(),
                                path: PathBuf::from("."),
                            },
                            0.1,          // 500 millicores as frac cores
                            10_u64 << 20, // 2GiB in bytes, KiB=<<10, MiB=<<20, GiB=<<30
                            None,
                            &NAMESPACE_LOOKUP_READ_ONLY,
                        )?
                        .into())
                    })
                    .collect::<Result<Vec<_>>>()?,
            )
            .await
    });

    ASYNC_RUNTIME.block_on(async { start_handle.await? })?;

    Ok(())
}
