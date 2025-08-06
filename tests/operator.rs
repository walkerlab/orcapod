#![expect(missing_docs, clippy::panic_in_result_fn, reason = "OK in tests.")]
pub mod fixture;
use orcapod::{
    core::operator::{JoinOperator, MapOperator, Operator as _},
    uniffi::{
        error::Result,
        model::packet::{Blob, BlobKind, Packet, PathSet, URI},
    },
};
use std::{collections::HashMap, path::PathBuf};

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
            "Expected packet {packet:?} not found in the vector."
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
        &operator.process_packets(input_streams).await?,
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
        &[
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
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn map_once() -> Result<()> {
    let operator = MapOperator::new(HashMap::from([("key_old".into(), "key_new".into())]));
    assert_contains_packet(
        &operator
            .process_packets(vec![(
                "parent".into(),
                Packet::from([
                    make_packet_key("key_old".into(), "some/key.txt".into()),
                    make_packet_key("subject".into(), "some/subject.txt".into()),
                ]),
            )])
            .await?,
        &[Packet::from([
            make_packet_key("key_new".into(), "some/key.txt".into()),
            make_packet_key("subject".into(), "some/subject.txt".into()),
        ])],
    );
    Ok(())
}
