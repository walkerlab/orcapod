#![expect(
    missing_docs,
    clippy::panic_in_result_fn,
    clippy::unwrap_used,
    clippy::panic,
    reason = "OK in tests."
)]
pub mod fixture;
use fixture::{TestDirs, combine_txt_pod};
use orcapod::{
    core::operator::{JoinOperator, MapOperator, Operator as _, PodOperator},
    uniffi::{
        error::Result,
        model::packet::{Blob, BlobKind, Packet, PathSet, URI},
        orchestrator::{agent::Agent, docker::LocalDockerOrchestrator},
    },
};
use pretty_assertions::assert_eq as pretty_assert_eq;
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::fs;

fn make_packet_key(key_name: String, filepath: String) -> (String, PathSet) {
    (
        key_name,
        PathSet::Unary(Blob {
            kind: BlobKind::File,
            location: URI {
                namespace: "default".into(),
                path: PathBuf::from(filepath),
            },
            checksum: String::new(),
        }),
    )
}

fn assert_contains_packet(packets_to_check: &Vec<Packet>, vec_to_check: &[Packet]) {
    for packet in packets_to_check {
        assert!(
            vec_to_check.contains(packet),
            "{}",
            format!("Expected packet {packet:?} not found in the vector.")
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn join_once() -> Result<()> {
    let operator = JoinOperator::new(2);

    let left_stream = (0..3)
        .map(|i| {
            (
                "left".into(),
                Packet::from([make_packet_key(
                    "subject".into(),
                    format!("left/subject{i}.png"),
                )]),
            )
        })
        .collect::<Vec<_>>();

    let right_stream = (0..2)
        .map(|i| {
            (
                "right".into(),
                Packet::from([make_packet_key(
                    "style".into(),
                    format!("right/style{i}.t7"),
                )]),
            )
        })
        .collect::<Vec<_>>();

    let mut input_streams = left_stream;
    input_streams.extend(right_stream);

    assert_contains_packet(
        &vec![
            Packet::from([
                make_packet_key("subject".into(), "left/subject0.png".into()),
                make_packet_key("style".into(), "right/style0.t7".into()),
            ]),
            Packet::from([
                make_packet_key("subject".into(), "left/subject0.png".into()),
                make_packet_key("style".into(), "right/style1.t7".into()),
            ]),
            Packet::from([
                make_packet_key("subject".into(), "left/subject1.png".into()),
                make_packet_key("style".into(), "right/style0.t7".into()),
            ]),
            Packet::from([
                make_packet_key("subject".into(), "left/subject2.png".into()),
                make_packet_key("style".into(), "right/style0.t7".into()),
            ]),
            Packet::from([
                make_packet_key("subject".into(), "left/subject1.png".into()),
                make_packet_key("style".into(), "right/style1.t7".into()),
            ]),
            Packet::from([
                make_packet_key("subject".into(), "left/subject2.png".into()),
                make_packet_key("style".into(), "right/style1.t7".into()),
            ]),
        ],
        &operator.process_packets(input_streams).await?,
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn join_spotty() -> Result<()> {
    let operator = JoinOperator::new(2);

    assert!(
        operator
            .process_packets(vec![(
                "right".into(),
                Packet::from([make_packet_key("style".into(), "right/style0.t7".into(),)]),
            )])
            .await?
            .is_empty(),
        "Unexpected streams."
    );

    assert!(
        operator
            .process_packets(vec![(
                "right".into(),
                Packet::from([make_packet_key("style".into(), "right/style1.t7".into(),)]),
            )])
            .await?
            .is_empty(),
        "Unexpected streams."
    );

    assert_contains_packet(
        &vec![
            Packet::from([
                make_packet_key("subject".into(), "left/subject0.png".into()),
                make_packet_key("style".into(), "right/style0.t7".into()),
            ]),
            Packet::from([
                make_packet_key("subject".into(), "left/subject0.png".into()),
                make_packet_key("style".into(), "right/style1.t7".into()),
            ]),
        ],
        &operator
            .process_packets(vec![(
                "left".into(),
                Packet::from([make_packet_key(
                    "subject".into(),
                    "left/subject0.png".into(),
                )]),
            )])
            .await?,
    );

    assert_contains_packet(
        &vec![
            Packet::from([
                make_packet_key("subject".into(), "left/subject1.png".into()),
                make_packet_key("style".into(), "right/style0.t7".into()),
            ]),
            Packet::from([
                make_packet_key("subject".into(), "left/subject1.png".into()),
                make_packet_key("style".into(), "right/style1.t7".into()),
            ]),
            Packet::from([
                make_packet_key("subject".into(), "left/subject2.png".into()),
                make_packet_key("style".into(), "right/style0.t7".into()),
            ]),
            Packet::from([
                make_packet_key("subject".into(), "left/subject2.png".into()),
                make_packet_key("style".into(), "right/style1.t7".into()),
            ]),
        ],
        &operator
            .process_packets(
                (1..3)
                    .map(|i| {
                        (
                            "left".into(),
                            Packet::from([make_packet_key(
                                "subject".into(),
                                format!("left/subject{i}.png"),
                            )]),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
            .await?,
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn map_once() -> Result<()> {
    let operator = MapOperator::new(HashMap::from([("key_old".into(), "key_new".into())]));

    assert_contains_packet(
        &vec![Packet::from([
            make_packet_key("key_new".into(), "some/key.txt".into()),
            make_packet_key("subject".into(), "some/subject.txt".into()),
        ])],
        &operator
            .process_packets(vec![(
                "parent".into(),
                Packet::from([
                    make_packet_key("key_old".into(), "some/key.txt".into()),
                    make_packet_key("subject".into(), "some/subject.txt".into()),
                ]),
            )])
            .await?,
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn combine_txt_pod_job() -> Result<()> {
    // Create the test_dir and get the namespace lookup
    let test_dirs = TestDirs::new(&HashMap::from([(
        "default".to_owned(),
        Some("./tests/extra/data/"),
    )]))?;
    let namespace_lookup = test_dirs.namespace_lookup();

    // Start an agent to process the orchestrator
    let (group, host) = ("combine_txt_pod_job", "host");
    let agent = Agent::new(
        group.to_owned(),
        host.to_owned(),
        LocalDockerOrchestrator::new()?.into(),
    )?;
    let agent_client_clone = Arc::clone(&agent.client);
    let namespace_lookup_clone = namespace_lookup.clone();
    let agent_join_handle =
        tokio::spawn(async move { agent.start(&namespace_lookup_clone, None).await });

    // Create a pod operator with some fake info for test
    let pod_operator = PodOperator::new(
        "test_node".into(),
        combine_txt_pod("test")?.into(),
        "default".into(),
        namespace_lookup.into(),
        agent_client_clone,
    );

    // Create input packet
    let packet = Packet::from([
        (
            "input_1".into(),
            PathSet::Unary(Blob::new(
                BlobKind::File,
                URI::new("default".into(), "input_txt/black.txt".into()),
            )),
        ),
        (
            "input_2".into(),
            PathSet::Unary(Blob::new(
                BlobKind::File,
                URI::new("default".into(), "input_txt/cat.txt".into()),
            )),
        ),
    ]);

    // Process the packet
    let output_packets = pod_operator
        .process_packets(vec![("pod_operator_test".into(), packet)])
        .await?;

    // Verify that the output file has been created and contains the expected content
    match output_packets.first().unwrap().get("output") {
        Some(PathSet::Unary(Blob { location, .. })) => {
            let file_content = fs::read_to_string(&location.path).await?;
            pretty_assert_eq!(file_content, "black\ncat\n");
        }
        _ => panic!("Output packet does not contain the expected output blob."),
    }

    // Stop the agent
    agent_join_handle.abort();

    Ok(())
}
