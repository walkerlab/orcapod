#![expect(
    missing_docs,
    clippy::panic_in_result_fn,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::get_unwrap,
    clippy::unwrap_used,
    reason = "OK in tests."
)]

pub mod fixture;
use fixture::pod_custom;
use indoc::indoc;
use orcapod::uniffi::{
    error::Result,
    model::{
        Annotation,
        packet::{Blob, BlobKind, PathInfo, PathSet, URI},
        pipeline::{NodeURI, Pipeline, PipelineJob},
    },
};
use std::collections::HashMap;

use crate::fixture::{NAMESPACE_LOOKUP_READ_ONLY, pipeline, pipeline_job};

#[test]
fn input_packet_checksum() -> Result<()> {
    let pipeline = Pipeline::new(
        indoc! {"
            digraph {
                A
            }
        "},
        HashMap::from([(
            "A".into(),
            pod_custom(
                "alpine:3.14",
                vec!["echo".into()],
                HashMap::from([(
                    "node_key_1".into(),
                    PathInfo {
                        path: "/tmp/input/subject.jpeg".into(),
                        match_pattern: r".*\.jpeg".into(),
                    },
                )]),
            )?
            .into(),
        )]),
        HashMap::from([(
            "pipeline_key_1".into(),
            vec![NodeURI {
                node_name: "A".into(),
                key: "node_key_1".into(),
            }],
        )]),
        HashMap::new(),
        None,
    )?;

    let pipeline_job = PipelineJob::new(
        pipeline.into(),
        &HashMap::from([(
            "pipeline_key_1".into(),
            vec![PathSet::Collection(vec![Blob {
                kind: BlobKind::File,
                location: URI {
                    namespace: "default".into(),
                    path: "images/subject.jpeg".into(),
                },
                checksum: String::new(),
            }])],
        )]),
        URI {
            namespace: "default".into(),
            path: "output/pipeline".into(),
        },
        None,
        &NAMESPACE_LOOKUP_READ_ONLY,
    )?;

    let checksum = match &pipeline_job.input_packet["pipeline_key_1"].first() {
        Some(PathSet::Collection(blobs)) => blobs[0].checksum.clone(),
        Some(_) | None => panic!("Input configuration unexpectedly changed."),
    };

    assert_eq!(
        checksum,
        "8b44b8ea83b1f5eec3ac16cf941767e629896c465803fb69c21adbbf984516bd".to_owned(),
        "Incorrect checksum"
    );
    Ok(())
}

#[test]
fn creation() -> Result<()> {
    // This test checks if the pipeline can be created successfully.
    let pipeline = pipeline()?;

    assert_eq!(
        pipeline.annotation,
        Some(Annotation {
            name: "Sentence making pipeline".to_owned(),
            description: "Parse txt files with txt and to form sentences".to_owned(),
            version: "1.0.0".to_owned(),
        }),
        "Pipeline annotation does not match expected values."
    );

    assert_eq!(
        pipeline.graph.node_count(),
        8,
        "Pipeline graph should have 8 nodes."
    );
    assert_eq!(
        pipeline.graph.edge_count(),
        7,
        "Pipeline graph should have 7 edges."
    );

    Ok(())
}

/// Verify that the utility function that computes the input packets to feed into each input node works as expected.
#[test]
fn get_input_packet_per_node() -> Result<()> {
    let pipeline_job = pipeline_job(&NAMESPACE_LOOKUP_READ_ONLY)?;

    let input_packets_per_node = pipeline_job.get_input_packet_per_node()?;

    // Given the pipeline definition used in pipeline_job, we expect the following input packets per node:
    // The full sentence to be constructed is "Where is the black/tabby cat hiding/playing"
    // Node A: 1 packets, with keys input "input_1" and "input_2" Due to only Where.txt and is_the.txt being route to this node
    // Node B: 2 packets, with keys "input_1" and "input_2" Due to black.txt / tabby.txt and cat.txt being routed to this node
    // Node C should not receive any input, as it is an internal node
    // Node pod_d_joiner: 2 packets, with keys "input_2" only, due to input_1 being received by the joiner node, and input_2 being hiding.txt

    // Check A
    let input_packet_node_a = input_packets_per_node.get("A").unwrap();
    assert_num_of_packets(input_packet_node_a.len(), 1);
    assert_contains_keys(&input_packet_node_a[0], &["input_1", "input_2"]);

    // Check B
    let input_packet_node_b = input_packets_per_node.get("B").unwrap();
    assert_num_of_packets(input_packet_node_b.len(), 2);
    assert_contains_keys(&input_packet_node_b[0], &["input_1", "input_2"]);
    assert_contains_keys(&input_packet_node_b[1], &["input_1", "input_2"]);

    // Check C
    assert!(
        !input_packets_per_node.contains_key("C"),
        "Node C should not have any input packets.",
    );

    // Check pod_d_joiner
    // Node node_d_joiner: 2 packets, with keys "input_2" only, due to input_1 being received by the joiner node, and input_2 being hiding.txt
    let input_packet_node_d = input_packets_per_node.get("pod_d_joiner").unwrap();
    assert_num_of_packets(input_packet_node_d.len(), 2);
    assert_contains_keys(&input_packet_node_d[0], &["input_2"]);
    assert_contains_keys(&input_packet_node_d[1], &["input_2"]);

    // Check D
    assert!(
        !input_packets_per_node.contains_key("D"),
        "Node D should not have any input packets.",
    );

    Ok(())
}

fn assert_num_of_packets(num_of_packets: usize, expected: usize) {
    assert!(
        num_of_packets == expected,
        "Expected {expected} packets, but got {num_of_packets}."
    );
}

fn assert_contains_keys(input_packet: &HashMap<String, PathSet>, keys: &[&str]) {
    for key in keys {
        assert!(
            input_packet.contains_key(*key),
            "Input packet should contain key '{key}'."
        );
    }
}
