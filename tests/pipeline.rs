#![expect(
    missing_docs,
    clippy::panic_in_result_fn,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::type_complexity,
    reason = "OK in tests."
)]

pub mod fixture;
use fixture::{NAMESPACE_LOOKUP_READ_ONLY, pod_custom};
use indoc::indoc;
use orcapod::uniffi::{
    error::Result,
    model::{
        packet::{Blob, BlobKind, PathInfo, PathSet, URI},
        pipeline::{Kernel, NodeURI, Pipeline, PipelineJob},
    },
};
use pretty_assertions::assert_eq;
use std::collections::HashMap;

use crate::fixture::{combine_txt_pod, pipeline};

#[expect(clippy::too_many_lines, reason = "Test code")]
#[test]
fn preprocessing() -> Result<()> {
    let pipeline = pipeline()?;

    // Assert that every node has a non-empty hash
    let node_hashes = pipeline
        .graph
        .node_indices()
        .map(|idx| {
            (
                pipeline.graph[idx].label.as_str(),
                pipeline.graph[idx].hash.as_str(),
            )
        })
        .collect::<HashMap<_, _>>();

    assert_eq!(
        node_hashes,
        HashMap::from([
            (
                "pod_c_joiner",
                "d2141ce0c203a8b556d7dbbbc6268ac4bbfa444748f92baff42235787f2b7550"
            ),
            (
                "B",
                "964ebb9ddd6bb7db56e53c19e9ac34dfd08779a656295b01e70b5973adc61103"
            ),
            (
                "C",
                "96b30227e0243f282f7a898bd85a246127e664635a3969577932d7653cfb79cb"
            ),
            (
                "pod_a_mapper",
                "83bd3d17026c882db6b6cca7ccca0173f478c11449cfa8bfb13a0518a7e5e32a"
            ),
            (
                "pod_b_mapper",
                "dd73cd3ab345917b25fc028131d83da7ce1c53702fcbabdd19b86a8bdde158b3"
            ),
            (
                "pod_d_mapper",
                "d37f595093e8f7235f97213b3f7ff88b12786e48ec4f22275018cc7d22c113f8"
            ),
            (
                "A",
                "8e43dbc9fd55fa7d1a36fc4a6c036f4113b7aa7fcf38646a2f2472bac6774962"
            ),
            (
                "E",
                "6ec68cc43ea15472731a318584cc8792fb2ff93c96fed6f3f998849b75976694"
            ),
            (
                "D",
                "04cb341a09eeb771846377405a5f33d011f99a7dfa4739fd7876a7e70c994e4e"
            ),
            (
                "pod_c_mapper",
                "240c8e7fa5e0bd88239aba625387ea495fc5323a5d4b6b519946b8f8b907ddf6"
            ),
            (
                "pod_e_joiner",
                "36f3e88889ecf89183205f340043de61f3c6a254026aae5aa1ce587a666e8c30"
            ),
        ]),
        "Node hashes did not match"
    );

    // Check if the input spec contains the correct node hashes
    assert_eq!(
        pipeline.input_spec,
        HashMap::from([
            (
                "the".into(),
                vec![NodeURI {
                    node_id: "964ebb9ddd6bb7db56e53c19e9ac34dfd08779a656295b01e70b5973adc61103"
                        .into(),
                    key: "input_1".into(),
                },]
            ),
            (
                "where".into(),
                vec![NodeURI {
                    node_id: "8e43dbc9fd55fa7d1a36fc4a6c036f4113b7aa7fcf38646a2f2472bac6774962"
                        .into(),
                    key: "input_1".into(),
                },]
            ),
            (
                "cat_color".into(),
                vec![NodeURI {
                    node_id: "964ebb9ddd6bb7db56e53c19e9ac34dfd08779a656295b01e70b5973adc61103"
                        .into(),
                    key: "input_2".into(),
                },]
            ),
            (
                "is".into(),
                vec![NodeURI {
                    node_id: "8e43dbc9fd55fa7d1a36fc4a6c036f4113b7aa7fcf38646a2f2472bac6774962"
                        .into(),
                    key: "input_2".into(),
                },]
            ),
            (
                "cat".into(),
                vec![NodeURI {
                    node_id: "04cb341a09eeb771846377405a5f33d011f99a7dfa4739fd7876a7e70c994e4e"
                        .into(),
                    key: "input_1".into(),
                },]
            ),
            (
                "action".into(),
                vec![NodeURI {
                    node_id: "04cb341a09eeb771846377405a5f33d011f99a7dfa4739fd7876a7e70c994e4e"
                        .into(),
                    key: "input_2".into(),
                },]
            ),
        ]),
        "Input spec did not match"
    );

    // Check if the output spec contain the correct node hashes
    assert_eq!(
        pipeline.output_spec,
        HashMap::from([(
            "output".into(),
            NodeURI {
                node_id: "6ec68cc43ea15472731a318584cc8792fb2ff93c96fed6f3f998849b75976694".into(),
                key: "output".into(),
            }
        ),]),
        "Output spec did not match"
    );

    Ok(())
}

#[test]
fn input_packet_checksum() -> Result<()> {
    let pipeline = Pipeline::new(
        indoc! {"
            digraph {
                A
            }
        "},
        &HashMap::from([(
            "A".into(),
            Kernel::Pod {
                pod: pod_custom(
                    "alpine:3.14",
                    &["echo".into()],
                    HashMap::from([(
                        "node_key_1".into(),
                        PathInfo {
                            path: "/tmp/input".into(),
                            match_pattern: r".*\.jpeg".into(),
                        },
                    )]),
                )?
                .into(),
            },
        )]),
        HashMap::from([(
            "pipeline_key_1".into(),
            vec![NodeURI {
                node_id: "A".into(),
                key: "node_key_1".into(),
            }],
        )]),
        HashMap::new(),
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

/// Testing invalid conditions to make sure validation works
fn basic_pipeline_components() -> Result<(
    String,
    HashMap<String, Kernel>,
    HashMap<String, Vec<NodeURI>>,
    HashMap<String, NodeURI>,
)> {
    let dot = indoc! {"
        digraph {
            A
        }
    "};

    let metadata = HashMap::from([("A".into(), combine_txt_pod("A")?.into())]);

    let input_spec = HashMap::from([
        (
            "input_1".into(),
            vec![NodeURI {
                node_id: "A".into(),
                key: "input_1".into(),
            }],
        ),
        (
            "input_2".into(),
            vec![NodeURI {
                node_id: "A".into(),
                key: "input_2".into(),
            }],
        ),
    ]);

    let output_spec = HashMap::from([(
        "output".into(),
        NodeURI {
            node_id: "A".into(),
            key: "output".into(),
        },
    )]);

    Ok((dot.to_owned(), metadata, input_spec, output_spec))
}

#[test]
fn invalid_input_spec() -> Result<()> {
    let (dot, metadata, _, output_spec) = basic_pipeline_components()?;

    // Test invalid node reference in input_spec
    assert!(
        Pipeline::new(
            &dot,
            &metadata,
            HashMap::from([(
                "input_1".into(),
                vec![NodeURI {
                    node_id: "B".into(),
                    key: "input_1".into(),
                }],
            )]),
            output_spec.clone(),
        )
        .is_err(),
        "Pipeline creation should have failed due to invalid input_spec"
    );

    // Test invalid key reference in input_spec
    assert!(
        Pipeline::new(
            &dot,
            &metadata,
            HashMap::from([(
                "input_1".into(),
                vec![NodeURI {
                    node_id: "A".into(),
                    key: "input_3".into(),
                }],
            )]),
            output_spec,
        )
        .is_err(),
        "Pipeline creation should have failed due to invalid input_spec"
    );

    Ok(())
}

#[test]
fn invalid_output_spec() -> Result<()> {
    let (dot, metadata, input_spec, _) = basic_pipeline_components()?;

    // Test invalid output_spec node reference
    assert!(
        Pipeline::new(
            &dot,
            &metadata,
            input_spec.clone(),
            HashMap::from([(
                "A".into(),
                NodeURI {
                    node_id: "B".into(),
                    key: "output".into(),
                }
            )]),
        )
        .is_err(),
        "Pipeline creation should have failed due to invalid output_spec"
    );

    // Test invalid output_spec key reference
    assert!(
        Pipeline::new(
            &dot,
            &metadata,
            input_spec,
            HashMap::from([(
                "A".into(),
                NodeURI {
                    node_id: "A".into(),
                    key: "output_dne".into(),
                }
            )]),
        )
        .is_err(),
        "Pipeline creation should have failed due to invalid output_spec"
    );

    Ok(())
}
