use crate::uniffi::error::{Kind, OrcaError};
use bollard::errors::Error as BollardError;
use glob;
use serde_json;
use serde_yaml;
use std::{
    fmt::{self, Display, Formatter},
    io, path,
};

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
