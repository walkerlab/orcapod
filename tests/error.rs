#![expect(missing_docs, clippy::panic_in_result_fn, reason = "OK in tests.")]

pub mod fixture;
use fixture::{NAMESPACE_LOOKUP_READ_ONLY, pipeline, pod_job_style, pod_style};
use glob::glob;
use orcapod::{
    core::{crypto::hash_file, pipeline::PipelineJob},
    uniffi::{
        error::{OrcaError, Result},
        model::{Annotation, Blob, BlobKind, Input, OrcaPath, PodJob},
        orchestrator::{Orchestrator as _, docker::LocalDockerOrchestrator},
    },
};
use serde_json;
use serde_yaml;
use std::{collections::HashMap, fs, ops::Deref as _, path::PathBuf, sync::Arc};
use tokio::net::unix::pipe;

fn contains_debug(error: impl Into<OrcaError>) -> bool {
    !format!("{:?}", error.into()).is_empty()
}

#[test]
fn external_bollard() -> Result<()> {
    let orch = LocalDockerOrchestrator::new()?;
    let mut pod_job = pod_job_style(&NAMESPACE_LOOKUP_READ_ONLY)?;
    let mut pod = pod_job.pod.deref().clone();
    pod.image = "nonexistent_image".to_owned();
    pod_job.pod = Arc::new(pod);
    assert!(
        orch.start_blocking(&NAMESPACE_LOOKUP_READ_ONLY, &pod_job)
            .is_err_and(contains_debug),
        "Did not raise a bollard error."
    );
    Ok(())
}

#[test]
fn external_glob() {
    assert!(
        glob("a**/b").is_err_and(contains_debug),
        "Did not raise a glob error."
    );
}

#[test]
fn external_io() {
    assert!(
        fs::read_to_string("nonexistent_file.txt").is_err_and(contains_debug),
        "Did not raise an I/O error."
    );
}

#[test]
fn external_path_prefix() {
    assert!(
        PathBuf::from("/fake/path")
            .strip_prefix("/missing/path")
            .is_err_and(contains_debug),
        "Did not raise a path prefix error."
    );
}

#[test]
fn external_json() {
    assert!(
        serde_json::from_str::<HashMap<String, String>>("{").is_err_and(contains_debug),
        "Did not raise a serde json error."
    );
}

#[test]
fn external_yaml() {
    assert!(
        serde_yaml::from_str::<HashMap<String, String>>(":").is_err_and(contains_debug),
        "Did not raise a serde yaml error."
    );
}

#[test]
fn internal_invalid_filepath() {
    assert!(
        hash_file("nonexistent_file.txt").is_err_and(contains_debug),
        "Did not raise an invalid filepath error."
    );
}

#[test]
fn internal_key_missing() {
    assert!(
        pod_job_style(&HashMap::new()).is_err_and(contains_debug),
        "Did not raise a key missing error."
    );
}

#[test]
fn invalid_pod_job_input_map() -> Result<()> {
    let pod_job = PodJob::new(
        Some(Annotation {
            name: "style-transfer".to_owned(),
            description: "This is an example pod job.".to_owned(),
            version: "0.1.0".to_owned(),
        }),
        pod_style()?.into(),
        HashMap::from([
            (
                "wrong_key".to_owned(),
                Input::Unary(Blob {
                    kind: BlobKind::File,
                    location: OrcaPath {
                        namespace: "default".to_owned(),
                        path: PathBuf::from("styles/mosaic.t7"),
                    },
                    checksum: String::new(),
                }),
            ),
            (
                "base-input".to_owned(),
                Input::Collection(vec![
                    Blob {
                        kind: BlobKind::File,
                        location: OrcaPath {
                            namespace: "default".to_owned(),
                            path: PathBuf::from("styles/style1.t7"),
                        },
                        checksum: String::new(),
                    },
                    Blob {
                        kind: BlobKind::File,
                        location: OrcaPath {
                            namespace: "default".to_owned(),
                            path: PathBuf::from("images/subject.jpeg"),
                        },
                        checksum: String::new(),
                    },
                ]),
            ),
        ]),
        OrcaPath {
            namespace: "default".to_owned(),
            path: PathBuf::from("output"),
        },
        0.5,         // 500 millicores as frac cores
        2_u64 << 30, // 2GiB in bytes, KiB=<<10, MiB=<<20, GiB=<<30
        Some(HashMap::from([
            ("ZZZ".to_owned(), "PLEASE".to_owned()),
            ("AAA".to_owned(), "SORT".to_owned()),
        ])),
        &NAMESPACE_LOOKUP_READ_ONLY,
    );

    assert!(
        pod_job.is_err_and(contains_debug),
        "Did not raise a pod job error."
    );

    Ok(())
}

#[test]
fn invalid_pipeline_job_input_map() -> Result<()> {
    let pipeline_job = PipelineJob::new(pipeline()?, HashMap::new(), None);

    assert!(
        pipeline_job.is_err_and(contains_debug),
        "Did not raise a pipeline job error."
    );

    Ok(())
}
