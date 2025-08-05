#![expect(missing_docs, reason = "OK in tests.")]

use orcapod::{
    core::operator::{JoinOperator, MapOperator, Operator as _},
    uniffi::model::packet::{Blob, BlobKind, Packet, PathSet, URI},
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
            "{}",
            format!("Expected packet {packet:?} not found in the vector.")
        );
    }
}

#[test]
fn join_once() {
    let mut operator = JoinOperator::new(2);

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
        &operator.next(input_streams).collect::<Vec<_>>(),
    );
}

#[test]
fn join_spotty() {
    let mut operator = JoinOperator::new(2);

    assert!(
        operator
            .next(vec![(
                "right".into(),
                Packet::from([make_packet_key("style".into(), "right/style0.t7".into(),)]),
            )])
            .next()
            .is_none(),
        "Unexpected streams."
    );

    assert!(
        operator
            .next(vec![(
                "right".into(),
                Packet::from([make_packet_key("style".into(), "right/style1.t7".into(),)]),
            )])
            .next()
            .is_none(),
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
            .next(vec![(
                "left".into(),
                Packet::from([make_packet_key(
                    "subject".into(),
                    "left/subject0.png".into(),
                )]),
            )])
            .collect::<Vec<_>>(),
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
            .next(
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
            .collect::<Vec<_>>(),
    );
}

#[test]
fn map_once() {
    let mut operator = MapOperator::new(HashMap::from([("key_old".into(), "key_new".into())]));

    assert_contains_packet(
        &vec![Packet::from([
            make_packet_key("key_new".into(), "some/key.txt".into()),
            make_packet_key("subject".into(), "some/subject.txt".into()),
        ])],
        &operator
            .next(vec![(
                "parent".into(),
                Packet::from([
                    make_packet_key("key_old".into(), "some/key.txt".into()),
                    make_packet_key("subject".into(), "some/subject.txt".into()),
                ]),
            )])
            .collect::<Vec<_>>(),
    );
}
