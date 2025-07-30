#![expect(
    missing_docs,
    clippy::panic_in_result_fn,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "OK in tests."
)]

pub mod fixture;
use fixture::pod_custom;
use indoc::indoc;
use orcapod::uniffi::{
    error::Result,
    model::{
        packet::{Blob, BlobKind, PathInfo, PathSet, URI},
        pipeline::{Kernel, Pipeline, PipelineJob, SpecURI},
    },
};
use std::collections::HashMap;

use crate::fixture::NAMESPACE_LOOKUP_READ_ONLY;

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
            Kernel::Pod {
                r#ref: pod_custom(
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
        &HashMap::from([(
            "pipeline_key_1".into(),
            vec![SpecURI {
                node: "A".into(),
                key: "node_key_1".into(),
            }],
        )]),
        &HashMap::new(),
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
        &URI {
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
