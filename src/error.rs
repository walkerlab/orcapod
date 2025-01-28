use bollard::errors::Error as BollardError;
use colored::Colorize;
use glob;
use regex;
use serde_json;
use serde_yaml;
use std::{
    fmt::{self, Display, Formatter},
    io,
    path::PathBuf,
    result,
};
use thiserror::Error;
/// Shorthand for a Result that returns an `OrcaError`.
pub type Result<T> = result::Result<T, OrcaError>;
/// Possible errors you may encounter.
#[derive(Error, Debug)]
pub(crate) enum Kind {
    #[error("File `{}` already exists.", path.to_string_lossy().bright_cyan())]
    FileExists { path: PathBuf },
    #[error("No annotation found for `{name}:{version}` {class}.")]
    NoAnnotationFound {
        class: String,
        name: String,
        version: String,
    },
    #[error("No known container names.")]
    NoContainerNames,
    #[error("Out of generated random names.")]
    GeneratedNamesOverflow,
    #[error("No corresponding pod run found for pod job (hash: {pod_job_hash}).")]
    NoMatchingPodRun { pod_job_hash: String },
    #[error("An invalid datetime was set for pod result for pod job (hash: {pod_job_hash}).")]
    InvalidPodResultTerminatedDatetime { pod_job_hash: String },
    #[error("Received an empty response when attempting to load the alternate container image file: {path}.")]
    EmptyResponseWhenLoadingContainerAltImage { path: PathBuf },
    #[error("No tags found in provided container alternate image: {path}.")]
    NoTagFoundInContainerAltImage { path: PathBuf },
    #[error(transparent)]
    GlobPatternError(#[from] glob::PatternError),
    #[error(transparent)]
    RegexError(#[from] regex::Error),
    #[error(transparent)]
    SerdeYamlError(#[from] serde_yaml::Error),
    #[error(transparent)]
    SerdeJsonError(#[from] serde_json::Error),
    #[error(transparent)]
    IoError(#[from] io::Error),
    #[error(transparent)]
    BollardError(#[from] BollardError),
}
/// A stable error API interface.
#[derive(Error, Debug)]
pub struct OrcaError {
    kind: Kind,
}
impl OrcaError {
    /// Returns `true` if the error was caused by an invalid model annotation.
    pub const fn is_invalid_annotation(&self) -> bool {
        matches!(self.kind, Kind::NoAnnotationFound { .. })
    }
    /// Returns `true` if the error was caused by querying a purged pod run.
    pub const fn is_purged_pod_run(&self) -> bool {
        matches!(self.kind, Kind::NoMatchingPodRun { .. })
    }
}
impl Display for OrcaError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)
    }
}
impl From<glob::PatternError> for OrcaError {
    fn from(error: glob::PatternError) -> Self {
        Self {
            kind: Kind::GlobPatternError(error),
        }
    }
}
impl From<regex::Error> for OrcaError {
    fn from(error: regex::Error) -> Self {
        Self {
            kind: Kind::RegexError(error),
        }
    }
}
impl From<serde_yaml::Error> for OrcaError {
    fn from(error: serde_yaml::Error) -> Self {
        Self {
            kind: Kind::SerdeYamlError(error),
        }
    }
}
impl From<serde_json::Error> for OrcaError {
    fn from(error: serde_json::Error) -> Self {
        Self {
            kind: Kind::SerdeJsonError(error),
        }
    }
}
impl From<io::Error> for OrcaError {
    fn from(error: io::Error) -> Self {
        Self {
            kind: Kind::IoError(error),
        }
    }
}
impl From<BollardError> for OrcaError {
    fn from(error: BollardError) -> Self {
        Self {
            kind: Kind::BollardError(error),
        }
    }
}
impl From<Kind> for OrcaError {
    fn from(kind: Kind) -> Self {
        Self { kind }
    }
}
