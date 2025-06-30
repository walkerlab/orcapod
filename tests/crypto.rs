#![expect(
    missing_docs,
    clippy::panic_in_result_fn,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "OK in tests."
)]
pub mod fixture;
use fixture::pod_style;

use orcapod::{
    core::crypto::{hash_buffer, hash_dir, hash_file},
    uniffi::{
        error::Result,
        model::{Annotation, Blob, BlobKind, PathSet, PodJob, URI},
    },
};
use std::{collections::HashMap, fs::read, path::PathBuf};

#[test]
fn consistent_hash() -> Result<()> {
    let filepath = "./tests/extra/data/images/subject.jpeg";
    assert_eq!(
        hash_file(filepath)?,
        hash_buffer(&read(filepath)?),
        "Checksum not consistent."
    );
    Ok(())
}

#[test]
fn complex_hash() -> Result<()> {
    let dirpath = "./tests/extra/data/images";
    assert_eq!(
        hash_dir(dirpath)?,
        "6c96a478ea25e34fab045bc82858a2980b2cfb22db32e83c01349a8e7ed3b42c".to_owned(),
        "Directory checksum didn't match."
    );
    Ok(())
}

#[test]
fn nested_dir_hash() -> Result<()> {
    let namespace_lookup = HashMap::from([("default".to_owned(), PathBuf::from("./tests"))]);

    let pod_job = PodJob::new(
        Some(Annotation {
            name: "style-transfer".to_owned(),
            description: "This is an example pod job.".to_owned(),
            version: "0.1.0".to_owned(),
        }),
        pod_style()?.into(),
        HashMap::from([(
            "nested_dir".to_owned(),
            PathSet::Unary(Blob {
                kind: BlobKind::Directory,
                location: URI {
                    namespace: "default".to_owned(),
                    path: "extra".into(),
                },
                checksum: String::new(),
            }),
        )]),
        URI {
            namespace: "default".to_owned(),
            path: PathBuf::from("output"),
        },
        0.5,         // 500 millicores as frac cores
        2_u64 << 30, // 2GiB in bytes, KiB=<<10, MiB=<<20, GiB=<<30
        Some(HashMap::from([
            ("ZZZ".to_owned(), "PLEASE".to_owned()),
            ("AAA".to_owned(), "SORT".to_owned()),
        ])),
        &namespace_lookup,
    )?;

    match &pod_job.input_packet["nested_dir"] {
        PathSet::Unary(blob) => {
            assert_eq!(
                blob.checksum,
                hash_dir("./tests/extra")?,
                "Checksum didn't match."
            );
        }
        PathSet::Collection(_) => panic!("Expected a Unary input."),
    }

    Ok(())
}
