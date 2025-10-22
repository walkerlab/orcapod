#![expect(
    clippy::field_scoped_visibility_modifiers,
    reason = "Needed since SNAFU dynamically generating selectors."
)]

use bollard::errors::Error as BollardError;
use dot_parser::ast::PestError;
use snafu::prelude::Snafu;
use std::{
    backtrace::{Backtrace, BacktraceStatus},
    collections::HashSet,
    error::Error,
    fmt::{self, Formatter},
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
        "Failed to extract run info from the container image file: {container_name}."
    ))]
    FailedToExtractRunInfo {
        container_name: String,
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
    #[snafu(display(
        "Failed to get label hash from file name: {file_name}. Split result by \"-\" didn't return hash"
    ))]
    FailedToGetLabelHashFromFileName {
        file_name: String,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("Incomplete {kind} packet. Missing `{missing_keys:?}` keys."))]
    IncompletePacket {
        kind: String,
        missing_keys: Vec<String>,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display(
        "Fail to start pod with container_name: {container_name} with error: {reason}"
    ))]
    FailedToStartPod {
        container_name: String,
        reason: String,
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
    #[snafu(display(
        "Node '{node_name}' was referenced in input_spec, but is not a node in the graph."
    ))]
    InvalidInputSpecNodeNotInGraph {
        node_name: String,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display(
        "Key '{key}' was referenced in input_spec for node '{node_name}', but is not a key in that node's input spec."
    ))]
    InvalidOutputSpecKeyNotInNode {
        node_name: String,
        key: String,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display(
        "Node '{node_name}' was referenced in output_spec, but is not a node in the graph."
    ))]
    InvalidOutputSpecNodeNotInGraph {
        node_name: String,
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
    /// Returns `true` if the error was caused by an invalid file or directory path.
    pub const fn is_failed_to_start_pod(&self) -> bool {
        matches!(self.kind, Kind::FailedToStartPod { .. })
    }
    /// Returns container name if the
    pub fn get_container_name(&self) -> Option<String> {
        if let Kind::FailedToStartPod { container_name, .. } = &self.kind {
            Some(container_name.clone())
        } else {
            None
        }
    }
}

impl From<BollardError> for OrcaError {
    fn from(error: BollardError) -> Self {
        Self {
            kind: Kind::BollardError {
                source: error.into(),
                backtrace: Some(Backtrace::capture()),
            },
        }
    }
}
impl From<chrono::ParseError> for OrcaError {
    fn from(error: chrono::ParseError) -> Self {
        Self {
            kind: Kind::ChronoParseError {
                source: error.into(),
                backtrace: Some(Backtrace::capture()),
            },
        }
    }
}
impl From<PestError> for OrcaError {
    fn from(error: PestError) -> Self {
        Self {
            kind: Kind::DOTError {
                source: error.into(),
                backtrace: Some(Backtrace::capture()),
            },
        }
    }
}
impl From<glob::PatternError> for OrcaError {
    fn from(error: glob::PatternError) -> Self {
        Self {
            kind: Kind::GlobPatternError {
                source: error.into(),
                backtrace: Some(Backtrace::capture()),
            },
        }
    }
}
impl From<io::Error> for OrcaError {
    fn from(error: io::Error) -> Self {
        Self {
            kind: Kind::IoError {
                source: error.into(),
                backtrace: Some(Backtrace::capture()),
            },
        }
    }
}
impl From<path::StripPrefixError> for OrcaError {
    fn from(error: path::StripPrefixError) -> Self {
        Self {
            kind: Kind::PathPrefixError {
                source: error.into(),
                backtrace: Some(Backtrace::capture()),
            },
        }
    }
}
impl From<serde_json::Error> for OrcaError {
    fn from(error: serde_json::Error) -> Self {
        Self {
            kind: Kind::SerdeJsonError {
                source: error.into(),
                backtrace: Some(Backtrace::capture()),
            },
        }
    }
}
impl From<serde_yaml::Error> for OrcaError {
    fn from(error: serde_yaml::Error) -> Self {
        Self {
            kind: Kind::SerdeYamlError {
                source: error.into(),
                backtrace: Some(Backtrace::capture()),
            },
        }
    }
}
impl From<task::JoinError> for OrcaError {
    fn from(error: task::JoinError) -> Self {
        Self {
            kind: Kind::TokioTaskJoinError {
                source: error.into(),
                backtrace: Some(Backtrace::capture()),
            },
        }
    }
}
impl From<Kind> for OrcaError {
    fn from(error: Kind) -> Self {
        Self { kind: error }
    }
}
fn format_stack(backtrace: Option<&Backtrace>) -> String {
    backtrace.map_or(
        String::new(),
        |unpacked_backtrace| match unpacked_backtrace.status() {
            BacktraceStatus::Captured => {
                format!("\nstack backtrace:\n{unpacked_backtrace}")
            }
            BacktraceStatus::Disabled | BacktraceStatus::Unsupported | _ => String::new(),
        },
    )
}
impl fmt::Debug for OrcaError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self.kind {
            Kind::AgentCommunicationFailure { backtrace, .. }
            | Kind::EmptyDir { backtrace, .. }
            | Kind::FailedToStartPod { backtrace, .. }
            | Kind::FailedToExtractRunInfo { backtrace, .. }
            | Kind::IncompletePacket { backtrace, .. }
            | Kind::InvalidPath { backtrace, .. }
            | Kind::InvalidIndex { backtrace, .. }
            | Kind::InvalidInputSpecNodeNotInGraph { backtrace, .. }
            | Kind::InvalidOutputSpecKeyNotInNode { backtrace, .. }
            | Kind::InvalidOutputSpecNodeNotInGraph { backtrace, .. }
            | Kind::KeyMissing { backtrace, .. }
            | Kind::MissingInfo { backtrace, .. }
            | Kind::FailedToGetLabelHashFromFileName { backtrace, .. }
            | Kind::FailedToGetPodJobOutput { backtrace, .. }
            | Kind::PipelineValidationErrorMissingKeys { backtrace, .. }
            | Kind::PodJobProcessingError { backtrace, .. }
            | Kind::PodJobSubmissionFailed { backtrace, .. }
            | Kind::UnexpectedPathType { backtrace, .. }
            | Kind::BollardError { backtrace, .. }
            | Kind::ChronoParseError { backtrace, .. }
            | Kind::DOTError { backtrace, .. }
            | Kind::GlobPatternError { backtrace, .. }
            | Kind::IoError { backtrace, .. }
            | Kind::PathPrefixError { backtrace, .. }
            | Kind::SerdeJsonError { backtrace, .. }
            | Kind::SerdeYamlError { backtrace, .. }
            | Kind::TokioTaskJoinError { backtrace, .. } => {
                write!(f, "{}{}", self.kind, format_stack(backtrace.as_ref()))
            }
        }
    }
}
