use crate::uniffi::error::OrcaError;
use bollard::errors::Error as BollardError;
use glob;
use serde_json;
use serde_yaml;
use std::{
    fmt::{self, Display, Formatter},
    io, path,
    path::PathBuf,
};
use thiserror::Error;
/// Possible errors you may encounter.
#[derive(Error, Debug)]
pub enum Kind {
    #[error(
        "Received an empty response when attempting to load the alternate container image file: {path}."
    )]
    EmptyResponseWhenLoadingContainerAltImage { path: PathBuf },
    #[error("Out of generated random names.")]
    GeneratedNamesOverflow,
    #[error("Path missing a file or directory name: {path}.")]
    InvalidPath { path: PathBuf },
    #[error("An invalid datetime was set for pod result for pod job (hash: {pod_job_hash}).")]
    InvalidPodResultTerminatedDatetime { pod_job_hash: String },
    #[error("Key '{key}' was not found in map.")]
    KeyMissing { key: String },
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
    #[error(transparent)]
    BollardError(#[from] BollardError),
    #[error(transparent)]
    GlobPatternError(#[from] glob::PatternError),
    #[error(transparent)]
    IoError(#[from] io::Error),
    #[error(transparent)]
    PathPrefixError(#[from] path::StripPrefixError),
    #[error(transparent)]
    SerdeJsonError(#[from] serde_json::Error),
    #[error(transparent)]
    SerdeYamlError(#[from] serde_yaml::Error),
}
impl Display for OrcaError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)
    }
}
impl From<BollardError> for OrcaError {
    fn from(error: BollardError) -> Self {
        Self {
            kind: Kind::BollardError(error),
        }
    }
}
impl From<glob::PatternError> for OrcaError {
    fn from(error: glob::PatternError) -> Self {
        Self {
            kind: Kind::GlobPatternError(error),
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
impl From<path::StripPrefixError> for OrcaError {
    fn from(error: path::StripPrefixError) -> Self {
        Self {
            kind: Kind::PathPrefixError(error),
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
impl From<serde_yaml::Error> for OrcaError {
    fn from(error: serde_yaml::Error) -> Self {
        Self {
            kind: Kind::SerdeYamlError(error),
        }
    }
}
impl From<Kind> for OrcaError {
    fn from(kind: Kind) -> Self {
        Self { kind }
    }
}
