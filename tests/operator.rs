#![expect(missing_docs, clippy::panic_in_result_fn, reason = "OK in tests.")]

use orcapod::{
    core::operator::{JoinOperator, Operator as _},
    uniffi::{
        error::Result,
        model::{Blob, BlobKind, Packet, PathSet, URI},
    },
};
use std::{collections::HashMap, path::PathBuf};

fn make_packet_key(key_name: String, filepath: String) -> (String, PathSet) {
    (
        key_name,
        PathSet::Unary {
            blob: Blob {
                kind: BlobKind::File,
                location: URI {
                    namespace: "default".into(),
                    path: PathBuf::from(filepath),
                },
                checksum: String::new(),
            },
        },
    )
}

#[test]
fn join_once() -> Result<()> {
    let mut operator = JoinOperator::new(2);

    let left_stream = (0..3)
        .map(|i| {
            (
                "left".into(),
                Packet(HashMap::from([make_packet_key(
                    "subject".into(),
                    format!("left/subject{i}.png"),
                )])),
            )
        })
        .collect::<Vec<_>>();

    let right_stream = (0..2)
        .map(|i| {
            (
                "right".into(),
                Packet(HashMap::from([make_packet_key(
                    "style".into(),
                    format!("right/style{i}.t7"),
                )])),
            )
        })
        .collect::<Vec<_>>();

    let mut input_streams = left_stream;
    input_streams.extend(right_stream);

    assert_eq!(
        operator.next(input_streams)?,
        vec![
            Packet(HashMap::from([
                make_packet_key("subject".into(), "left/subject0.png".into()),
                make_packet_key("style".into(), "right/style0.t7".into()),
            ])),
            Packet(HashMap::from([
                make_packet_key("subject".into(), "left/subject1.png".into()),
                make_packet_key("style".into(), "right/style0.t7".into()),
            ])),
            Packet(HashMap::from([
                make_packet_key("subject".into(), "left/subject2.png".into()),
                make_packet_key("style".into(), "right/style0.t7".into()),
            ])),
            Packet(HashMap::from([
                make_packet_key("subject".into(), "left/subject0.png".into()),
                make_packet_key("style".into(), "right/style1.t7".into()),
            ])),
            Packet(HashMap::from([
                make_packet_key("subject".into(), "left/subject1.png".into()),
                make_packet_key("style".into(), "right/style1.t7".into()),
            ])),
            Packet(HashMap::from([
                make_packet_key("subject".into(), "left/subject2.png".into()),
                make_packet_key("style".into(), "right/style1.t7".into()),
            ])),
        ],
        "Unexpected streams."
    );

    Ok(())
}

#[test]
fn join_spotty() -> Result<()> {
    let mut operator = JoinOperator::new(2);

    assert_eq!(
        operator.next(vec![(
            "right".into(),
            Packet(HashMap::from([make_packet_key(
                "style".into(),
                "right/style0.t7".into(),
            )])),
        )])?,
        vec![],
        "Unexpected streams."
    );

    assert_eq!(
        operator.next(vec![(
            "right".into(),
            Packet(HashMap::from([make_packet_key(
                "style".into(),
                "right/style1.t7".into(),
            )])),
        )])?,
        vec![],
        "Unexpected streams."
    );

    assert_eq!(
        operator.next(vec![(
            "left".into(),
            Packet(HashMap::from([make_packet_key(
                "subject".into(),
                "left/subject0.png".into(),
            )])),
        )])?,
        vec![
            Packet(HashMap::from([
                make_packet_key("subject".into(), "left/subject0.png".into()),
                make_packet_key("style".into(), "right/style0.t7".into()),
            ])),
            Packet(HashMap::from([
                make_packet_key("subject".into(), "left/subject0.png".into()),
                make_packet_key("style".into(), "right/style1.t7".into()),
            ])),
        ],
        "Unexpected streams."
    );

    assert_eq!(
        operator.next(
            (1..3)
                .map(|i| {
                    (
                        "left".into(),
                        Packet(HashMap::from([make_packet_key(
                            "subject".into(),
                            format!("left/subject{i}.png"),
                        )])),
                    )
                })
                .collect::<Vec<_>>()
        )?,
        vec![
            Packet(HashMap::from([
                make_packet_key("subject".into(), "left/subject1.png".into()),
                make_packet_key("style".into(), "right/style0.t7".into()),
            ])),
            Packet(HashMap::from([
                make_packet_key("subject".into(), "left/subject1.png".into()),
                make_packet_key("style".into(), "right/style1.t7".into()),
            ])),
            Packet(HashMap::from([
                make_packet_key("subject".into(), "left/subject2.png".into()),
                make_packet_key("style".into(), "right/style0.t7".into()),
            ])),
            Packet(HashMap::from([
                make_packet_key("subject".into(), "left/subject2.png".into()),
                make_packet_key("style".into(), "right/style1.t7".into()),
            ])),
        ],
        "Unexpected streams."
    );

    Ok(())
}
