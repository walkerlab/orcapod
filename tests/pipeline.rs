#![expect(
    missing_docs,
    clippy::panic_in_result_fn,
    clippy::panic,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    reason = "OK in tests."
)]

pub mod fixture;
use fixture::{TestDirs, pull_image};
use indoc::{formatdoc, indoc};
use orcapod::uniffi::{
    error::Result,
    model::{
        Blob, BlobKind, InputSpecURI, Kernel, OutputSpecURI, PathInfo, PathSet, Pipeline,
        PipelineJob, Pod, URI,
    },
    orchestrator::{
        agent::{Agent, AgentClient},
        docker::LocalDockerOrchestrator,
    },
    pipeline::PipelineStatus,
    store::filestore::LocalFileStore,
};
use std::{collections::HashMap, fs, path::PathBuf, sync::Arc, time::Duration};
use tokio::{task::JoinSet, time::sleep as async_sleep};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn adder() -> Result<()> {
    let test_dirs = TestDirs::new(&HashMap::from([("default".to_owned(), None::<String>)]))?;
    let data_dir = test_dirs.namespace_lookup()["default"].join("data");
    let store = LocalFileStore::new(test_dirs.namespace_lookup()["default"].join("store"));
    // config
    let margin_millis = 6;
    let (group, host) = ("pipeline_adder", "host");
    fs::create_dir_all(data_dir.join("input"))?;
    for i in 1..16 {
        fs::write(data_dir.join(format!("input/{i}.txt")), i.to_string())?;
    }
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
        let namespace_lookup = test_dirs.namespace_lookup();
        async move {
            inner_agent
                .start(&namespace_lookup, Some(inner_store.into()))
                .await
        }
    });
    // setup pod
    let image_reference = "alpine:3.14";
    pull_image(image_reference)?;
    let run_duration_secs = 5;
    let adder_pod = Pod::new(
        image_reference.to_owned(),
        vec![
            "sh".into(),
            "-c".into(),
            formatdoc! {"
                echo 'Adding two numbers'
                echo $(($(cat /tmp/input/left.txt) + $(cat /tmp/input/right.txt))) > /tmp/output/answer.txt
                sleep {run_duration_secs}
            "}.trim().replace('\n', " && "),
        ],
        HashMap::from([
            (
                "left".to_owned(),
                PathInfo {
                    path: PathBuf::from("/tmp/input/left.txt"),
                    match_pattern: r".*\.txt".to_owned(),
                },
            ),
            (
                "right".to_owned(),
                PathInfo {
                    path: PathBuf::from("/tmp/input/right.txt"),
                    match_pattern: r".*\.txt".to_owned(),
                },
            ),
        ]),
        PathBuf::from("/tmp/output"),
        HashMap::from([
            (
                "answer".to_owned(),
                PathInfo {
                    path: PathBuf::from("answer.txt"),
                    match_pattern: r".*\.txt".to_owned(),
                },
            ),
        ]),
        "https://github.com/user/style-transfer/tree/1.0.0".to_owned(),
        0.1,        // 100 millicores as frac cores
        10_u64 << 20, // 10MiB in bytes
        None,
        None,
    )?;
    // setup pipeline
    let pipeline = Pipeline::new(
        indoc! {"
            digraph {
                add_a -> map_left_a
                add_b -> map_right_a
                add_c -> map_left_b
                add_d -> map_right_b
                { map_left_a map_right_a } -> cartesian_a -> add_e -> map_left_c
                { map_left_b map_right_b } -> cartesian_b -> add_f -> map_right_c
                { map_left_c map_right_c } -> cartesian_c -> add_g
            }
        "},
        ["a", "b", "c"]
            .into_iter()
            .flat_map(|k| ["left", "right"].into_iter().map(move |side| (k, side)))
            .map(|(k, side)| {
                (
                    format!("map_{side}_{k}"),
                    Kernel::MapOperator {
                        map: HashMap::from([("answer".into(), (*side).into())]),
                    },
                )
            })
            .chain(
                ["a", "b", "c"]
                    .iter()
                    .map(|k| (format!("cartesian_{k}"), Kernel::JoinOperator)),
            )
            .chain(["a", "b", "c", "d", "e", "f", "g"].iter().map(|k| {
                (
                    format!("add_{k}"),
                    Kernel::Pod {
                        r#ref: adder_pod.clone().into(),
                    },
                )
            }))
            .collect(),
        &["a", "b", "c", "d"]
            .into_iter()
            .flat_map(|k| ["left", "right"].into_iter().map(move |side| (k, side)))
            .map(|(k, side)| {
                (
                    format!("{side}_add_{k}"),
                    vec![InputSpecURI {
                        node: format!("add_{k}"),
                        key: (*side).into(),
                    }],
                )
            })
            .collect(),
        &HashMap::from([(
            "answer".into(),
            OutputSpecURI {
                node: "add_g".into(),
                key: "answer".into(),
            },
        )]),
    )?;
    assert!(
        pipeline.make_dot(true)?.contains("bold"),
        "Pipeline DAG does not include style."
    );
    let pipeline_job = PipelineJob::new(
        pipeline.into(),
        &[
            (
                "left_add_a".into(),
                (1..4)
                    .map(|i| PathSet::Unary {
                        blob: Blob {
                            kind: BlobKind::File,
                            location: URI {
                                namespace: "default".into(),
                                path: data_dir.join(format!("input/{i}.txt")),
                            },
                            checksum: String::new(),
                        },
                    })
                    .collect(),
            ),
            (
                "right_add_a".into(),
                (4..6)
                    .map(|i| PathSet::Unary {
                        blob: Blob {
                            kind: BlobKind::File,
                            location: URI {
                                namespace: "default".into(),
                                path: data_dir.join(format!("input/{i}.txt")),
                            },
                            checksum: String::new(),
                        },
                    })
                    .collect(),
            ),
        ]
        .into_iter()
        .chain(
            [
                "left_add_b",
                "right_add_b",
                "left_add_c",
                "right_add_c",
                "left_add_d",
                "right_add_d",
            ]
            .iter()
            .enumerate()
            .map(|(i, k)| {
                (
                    (*k).into(),
                    vec![PathSet::Unary {
                        blob: Blob {
                            kind: BlobKind::File,
                            location: URI {
                                namespace: "default".into(),
                                path: data_dir.join(format!("input/{}.txt", i + 6)),
                            },
                            checksum: String::new(),
                        },
                    }],
                )
            }),
        )
        .collect(),
        &URI {
            namespace: "default".into(),
            path: PathBuf::from("data/output/pipeline_adder"),
        },
        &test_dirs.namespace_lookup(),
    )?;
    // submit request
    services.spawn(async move {
        let pipeline_run = client.start_pipeline_job(pipeline_job.into()).await?;
        assert_eq!(
            pipeline_run.status(),
            PipelineStatus::Running,
            "Pipeline not running."
        );
        async_sleep(Duration::from_secs(10)).await; // wait until some progress
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
            pipeline_result.status,
            PipelineStatus::Completed,
            "Pipeline failed."
        );
        assert!(
            pipeline_run_pointer
                .summarize_dot()?
                .contains("Pipeline not active"),
            "Pipeline summary report incorrect."
        );
        let actual_runtime = pipeline_result.terminated - pipeline_result.created;
        let expected_runtime = 15 + margin_millis;
        assert!(
            actual_runtime <= expected_runtime,
            "Pipeline took too long (expected={expected_runtime}, actual={actual_runtime})."
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
