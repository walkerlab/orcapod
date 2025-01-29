use colored::Colorize;
use glob;
use regex;
use serde_yaml;
use std::{
    fmt,
    fmt::{Display, Formatter},
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
    #[error("{}", err_msg.bright_cyan())]
    InvalidURIForFileStore { err_msg: String },
    #[error("No annotation found for `{name}:{version}` {class}.")]
    NoAnnotationFound {
        class: String,
        name: String,
        version: String,
    },
    #[error("Multiple hash found for annotation: (name: {}, ver: {})", name.bright_cyan(), ver.bright_cyan())]
    MultipleHashFound { name: String, ver: String },
    #[error("Path: {} is an unsupported path type", path.to_string_lossy().bright_cyan())]
    UnsupportedPath { path: PathBuf },
    #[error("Unsupported data storage type: {}", data_storage_type.bright_cyan())]
    UnsupportedFileStorage { data_storage_type: String },
    #[error(transparent)]
    GlobPatternError(#[from] glob::PatternError),
    #[error(transparent)]
    RegexError(#[from] regex::Error),
    #[error(transparent)]
    SerdeYamlError(#[from] serde_yaml::Error),
    #[error(transparent)]
    IoError(#[from] io::Error),
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
impl From<io::Error> for OrcaError {
    fn from(error: io::Error) -> Self {
        Self {
            kind: Kind::IoError(error),
        }
    }
}
impl From<Kind> for OrcaError {
    fn from(kind: Kind) -> Self {
        Self { kind }
    }
}
