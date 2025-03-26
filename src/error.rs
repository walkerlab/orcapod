use bollard::errors::Error as BollardError;
use colored::Colorize as _;
use glob;
use regex;
use serde_json;
use serde_yaml;
use std::{
    fmt::{self, Display, Formatter},
    io, path,
    path::PathBuf,
    result,
    string::FromUtf8Error,
};
use thiserror::Error;

/// Shorthand for a Result that returns an `OrcaError`.
pub type Result<T> = result::Result<T, OrcaError>;
/// Possible errors you may encounter.
#[derive(Error, Debug)]
pub(crate) enum Kind {
    #[error("Received an empty response when attempting to load the alternate container image file: {path}.")]
    EmptyResponseWhenLoadingContainerAltImage { path: PathBuf },
    #[error("fail to extract file name for path; {}", path.to_string_lossy().bright_cyan())]
    FailedToExtractFileName { path: PathBuf },
    #[error("Out of generated random names.")]
    GeneratedNamesOverflow,
    #[error("Input file or folder at path {path} not found")]
    InputFileOrFolderNotFound { path: PathBuf },
    #[error("An invalid datetime was set for pod result for pod job (hash: {pod_job_hash}).")]
    InvalidPodResultTerminatedDatetime { pod_job_hash: String },
    #[error("IO Error: {} for path: {}", error, path.to_string_lossy())]
    IoErrorWithPath { error: io::Error, path: PathBuf },
    #[error("{}{}{}", "Key ".bright_red(), key, "was not found in map".bright_red())]
    KeyWasNotFoundError { key: String },
    #[error("Multiple hash found for {} and {}", name, version)]
    MultipleHashFound { name: String, version: String },
    #[error("No annotation found for `{name}:{version}` {class}.")]
    NoAnnotationFound {
        class: String,
        name: String,
        version: String,
    },
    #[error("No known container names.")]
    NoContainerNames,
    #[error("No corresponding pod run found for pod job (hash: {pod_job_hash}).")]
    NoMatchingPodRun { pod_job_hash: String },
    #[error("No tags found in provided container alternate image: {path}.")]
    NoTagFoundInContainerAltImage { path: PathBuf },
    #[error("Namespace {} not found", namespace.bright_cyan())]
    NameSpaceNotFound { namespace: String },

    #[error(transparent)]
    BollardError(#[from] BollardError),
    #[error(transparent)]
    FromUtf8Error(#[from] FromUtf8Error),
    #[error(transparent)]
    GlobPatternError(#[from] glob::PatternError),
    #[error(transparent)]
    IoError(#[from] io::Error),
    #[error(transparent)]
    PathPrefixError(#[from] path::StripPrefixError),
    #[error(transparent)]
    RegexError(#[from] regex::Error),
    #[error(transparent)]
    SerdeJsonError(#[from] serde_json::Error),
    #[error(transparent)]
    SerdeYamlError(#[from] serde_yaml::Error),
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

/// Resrot the functions TODO
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
impl From<FromUtf8Error> for OrcaError {
    fn from(error: FromUtf8Error) -> Self {
        Self {
            kind: Kind::FromUtf8Error(error),
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
impl From<path::StripPrefixError> for OrcaError {
    fn from(error: path::StripPrefixError) -> Self {
        Self {
            kind: Kind::PathPrefixError(error),
        }
    }
}
impl From<Kind> for OrcaError {
    fn from(kind: Kind) -> Self {
        Self { kind }
    }
}
