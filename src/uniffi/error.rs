#![expect(
    clippy::field_scoped_visibility_modifiers,
    reason = "Needed since SNAFU dynamically generating selectors."
)]

use bollard::errors::Error as BollardError;
use glob;
use serde_json;
use serde_yaml;
use snafu::prelude::Snafu;
use std::{
    backtrace::Backtrace,
    collections::HashMap,
    io,
    path::{self, PathBuf},
    result,
};
use tokio::task::JoinError;
use uniffi;

use super::model::Input;
/// Shorthand for a Result that returns an `OrcaError`.
pub type Result<T, E = OrcaError> = result::Result<T, E>;
/// Possible errors you may encounter.
#[derive(Snafu, Debug, uniffi::Error)]
#[snafu(module(selector), visibility(pub(crate)), context(suffix(false)))]
#[uniffi(flat_error)]
pub(crate) enum Kind {
    EmptyResponseWhenLoadingContainerAltImage {
        path: PathBuf,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display(
        "Failed to extract run info from the container image file: {container_name}."
    ))]
    FailedToExtractRunInfo {
        container_name: String,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display(
        "Fail to start pod with container_name: {container_name} with error: {source}"
    ))]
    FailedToStartPod {
        container_name: String,
        source: BollardError,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("Out of generated random names."))]
    GeneratedNamesOverflow { backtrace: Option<Backtrace> },
    #[snafu(display("{source} ({path:?})."))]
    InvalidFileOrDirPath {
        path: PathBuf,
        source: io::Error,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display(
        "An invalid datetime was set for pod result for pod job (hash: {pod_job_hash})."
    ))]
    InvalidPodResultTerminatedDatetime {
        pod_job_hash: String,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("Key '{key}' was not found in map."))]
    KeyMissing {
        key: String,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("No annotation found for `{name}:{version}` {class}."))]
    NoAnnotationFound {
        class: String,
        name: String,
        version: String,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("No known container names."))]
    NoContainerNames { backtrace: Option<Backtrace> },
    #[snafu(display("Invalid parent node key: {parent_node_key}."))]
    NodeNotFound {
        parent_node_key: String,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("Missing file or directory name ({path:?})."))]
    NoFileName {
        path: PathBuf,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("No corresponding pod run found for pod job (hash: {pod_job_hash})."))]
    NoMatchingPodRun {
        pod_job_hash: String,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("No tags found in provided container alternate image: {path:?}."))]
    NoTagFoundInContainerAltImage {
        path: PathBuf,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("Input map {input_map:?} missing required stream_key {missing_keys:?}"))]
    MissingStreamKey {
        input_map: HashMap<String, Input>,
        missing_keys: Vec<String>,
        backtrace: Option<Backtrace>,
    },
    #[snafu(transparent)]
    BollardError {
        source: BollardError,
        backtrace: Option<Backtrace>,
    },
    #[snafu(transparent)]
    GlobPatternError {
        source: glob::PatternError,
        backtrace: Option<Backtrace>,
    },
    #[snafu(transparent)]
    IoError {
        source: io::Error,
        backtrace: Option<Backtrace>,
    },
    #[snafu(transparent)]
    PathPrefixError {
        source: path::StripPrefixError,
        backtrace: Option<Backtrace>,
    },
    #[snafu(transparent)]
    SerdeJsonError {
        source: serde_json::Error,
        backtrace: Option<Backtrace>,
    },
    #[snafu(transparent)]
    SerdeYamlError {
        source: serde_yaml::Error,
        backtrace: Option<Backtrace>,
    },
    #[snafu(transparent)]
    TokioJoinError {
        source: JoinError,
        backtrace: Option<Backtrace>,
    },
}
/// A stable error API interface.
#[derive(Snafu, uniffi::Object)]
#[snafu(display("{self:?}"))]
#[uniffi::export(Display)]
pub struct OrcaError {
    pub(crate) kind: Kind,
}

#[uniffi::export]
impl OrcaError {
    /// Returns `true` if the error was caused by an invalid model annotation.
    pub const fn is_invalid_annotation(&self) -> bool {
        matches!(self.kind, Kind::NoAnnotationFound { .. })
    }
    /// Returns `true` if the error was caused by querying a purged pod run.
    pub const fn is_purged_pod_run(&self) -> bool {
        matches!(self.kind, Kind::NoMatchingPodRun { .. })
    }
    /// Returns `true` if the error was caused by an invalid file or directory path.
    pub const fn is_failed_to_start_pod(&self) -> bool {
        matches!(self.kind, Kind::FailedToStartPod { .. })
    }
}
