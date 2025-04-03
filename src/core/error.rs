use crate::uniffi::error::OrcaError;
use bollard::errors::Error as BollardError;
use glob;
use serde_json;
use serde_yaml;
use snafu::prelude::Snafu;
use std::{
    backtrace::{Backtrace, BacktraceStatus},
    fmt::{self, Formatter},
    io,
    path::{self, PathBuf},
};
/// Possible errors you may encounter.
#[derive(Snafu, Debug)]
#[snafu(module(selector), visibility(pub), context(suffix(false)))]
pub enum Kind {
    #[snafu(display(
        "Received an empty response when attempting to load the alternate container image file: {path:?}."
    ))]
    EmptyResponseWhenLoadingContainerAltImage {
        path: PathBuf,
        backtrace: Option<Backtrace>,
    },
    #[snafu(display("Out of generated random names."))]
    GeneratedNamesOverflow { backtrace: Option<Backtrace> },
    #[snafu(display("{source} ({path:?})."))]
    InvalidFilepath {
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
}
impl From<BollardError> for OrcaError {
    fn from(error: BollardError) -> Self {
        Self(Kind::BollardError {
            source: error,
            backtrace: Some(Backtrace::capture()),
        })
    }
}
impl From<glob::PatternError> for OrcaError {
    fn from(error: glob::PatternError) -> Self {
        Self(Kind::GlobPatternError {
            source: error,
            backtrace: Some(Backtrace::capture()),
        })
    }
}
impl From<io::Error> for OrcaError {
    fn from(error: io::Error) -> Self {
        Self(Kind::IoError {
            source: error,
            backtrace: Some(Backtrace::capture()),
        })
    }
}
impl From<path::StripPrefixError> for OrcaError {
    fn from(error: path::StripPrefixError) -> Self {
        Self(Kind::PathPrefixError {
            source: error,
            backtrace: Some(Backtrace::capture()),
        })
    }
}
impl From<serde_json::Error> for OrcaError {
    fn from(error: serde_json::Error) -> Self {
        Self(Kind::SerdeJsonError {
            source: error,
            backtrace: Some(Backtrace::capture()),
        })
    }
}
impl From<serde_yaml::Error> for OrcaError {
    fn from(error: serde_yaml::Error) -> Self {
        Self(Kind::SerdeYamlError {
            source: error,
            backtrace: Some(Backtrace::capture()),
        })
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
        match &self.0 {
            Kind::EmptyResponseWhenLoadingContainerAltImage { backtrace, .. }
            | Kind::GeneratedNamesOverflow { backtrace, .. }
            | Kind::InvalidFilepath { backtrace, .. }
            | Kind::InvalidPodResultTerminatedDatetime { backtrace, .. }
            | Kind::KeyMissing { backtrace, .. }
            | Kind::NoAnnotationFound { backtrace, .. }
            | Kind::NoContainerNames { backtrace, .. }
            | Kind::NoFileName { backtrace, .. }
            | Kind::NoMatchingPodRun { backtrace, .. }
            | Kind::NoTagFoundInContainerAltImage { backtrace, .. }
            | Kind::BollardError { backtrace, .. }
            | Kind::GlobPatternError { backtrace, .. }
            | Kind::IoError { backtrace, .. }
            | Kind::PathPrefixError { backtrace, .. }
            | Kind::SerdeJsonError { backtrace, .. }
            | Kind::SerdeYamlError { backtrace, .. } => {
                write!(f, "{}{}", self.0, format_stack(backtrace.as_ref()))
            }
        }
    }
}
