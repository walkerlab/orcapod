#![expect(
    clippy::field_scoped_visibility_modifiers,
    reason = "Needed since SNAFU dynamically generating selectors."
)]

use bollard::errors::Error as BollardError;
use dot_parser::ast::PestError;
use glob;
use serde_json;
use serde_yaml;
use snafu::prelude::Snafu;
use std::{
    backtrace::Backtrace,
    collections::HashSet,
    error::Error,
    io,
    path::{self, PathBuf},
    result,
};
use tokio::task;
use uniffi;

/// Shorthand for a Result that returns an [`OrcaError`].
pub type Result<T, E = OrcaError> = result::Result<T, E>;
/// Possible errors you may encounter.
#[derive(Snafu, Debug, uniffi::Error)]
#[snafu(module(selector), visibility(pub(crate)), context(suffix(false)))]
#[uniffi(flat_error)]
pub(crate) enum Kind {
    #[snafu(display("Agent encountered a communication error. Reason: {source}."))]
    AgentCommunicationFailure {
        source: Box<dyn Error + Send + Sync>,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("Empty directory: {dir:?}, where they should be files"))]
    EmptyDir {
        dir: PathBuf,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display(
        "Missing expected output file or dir with key {packet_key} at path {path:?} for pod job (hash: {pod_job_hash})."
    ))]
    FailedToGetPodJobOutput {
        pod_job_hash: String,
        packet_key: String,
        path: Box<PathBuf>,
        io_error: Box<io::Error>,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("Incomplete {kind} packet. Missing `{missing_keys:?}` keys."))]
    IncompletePacket {
        kind: String,
        missing_keys: Vec<String>,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("{source} ({path:?})."))]
    InvalidPath {
        path: PathBuf,
        source: io::Error,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("Failed to get items at idx {idx}."))]
    InvalidIndex {
        idx: usize,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("Key '{key}' was not found in map."))]
    KeyMissing {
        key: String,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("Missing info. Details: {details}."))]
    MissingInfo {
        details: String,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("Node '{node_name}' is missing required keys: {missing_keys:?}."))]
    PipelineValidationErrorMissingKeys {
        node_name: String,
        missing_keys: HashSet<String>,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("Pod job submission failed with reason: {reason}."))]
    PodJobSubmissionFailed {
        reason: String,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("Pod job {hash} failed to process with reason: {reason}."))]
    PodJobProcessingError {
        hash: String,
        reason: String,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("Unexpected path type: {path:?}. Only support files and directories."))]
    UnexpectedPathType {
        path: PathBuf,
        backtrace: Option<Backtrace>,
    },
    #[snafu(transparent)]
    BollardError {
        source: Box<BollardError>,
        backtrace: Option<Backtrace>,
    },
    #[snafu(transparent)]
    ChronoParseError {
        source: Box<chrono::ParseError>,
        backtrace: Option<Backtrace>,
    },
    #[snafu(transparent)]
    DOTError {
        source: Box<PestError>,
        backtrace: Option<Backtrace>,
    },
    #[snafu(transparent)]
    GlobPatternError {
        source: Box<glob::PatternError>,
        backtrace: Option<Backtrace>,
    },
    #[snafu(transparent)]
    IoError {
        source: Box<io::Error>,
        backtrace: Option<Backtrace>,
    },
    #[snafu(transparent)]
    PathPrefixError {
        source: Box<path::StripPrefixError>,
        backtrace: Option<Backtrace>,
    },
    #[snafu(transparent)]
    SerdeJsonError {
        source: Box<serde_json::Error>,
        backtrace: Option<Backtrace>,
    },
    #[snafu(transparent)]
    SerdeYamlError {
        source: Box<serde_yaml::Error>,
        backtrace: Option<Backtrace>,
    },
    #[snafu(transparent)]
    TokioTaskJoinError {
        source: Box<task::JoinError>,
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
    pub fn is_invalid_annotation(&self) -> bool {
        matches!(&self.kind, Kind::MissingInfo { details, .. } if details.contains("annotation"))
    }
    /// Returns `true` if the error was caused by querying a purged pod run.
    pub fn is_purged_pod_run(&self) -> bool {
        matches!(&self.kind, Kind::MissingInfo { details, .. } if details.contains("pod run"))
    }
}
