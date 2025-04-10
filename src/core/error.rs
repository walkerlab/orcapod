use crate::uniffi::error::{Kind, OrcaError};
use bollard::errors::Error as BollardError;
use glob;
use serde_json;
use serde_yaml;
use std::{
    backtrace::{Backtrace, BacktraceStatus},
    fmt::{self, Formatter},
    io,
    path::{self},
    string::FromUtf8Error,
};

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
impl From<FromUtf8Error> for OrcaError {
    fn from(error: FromUtf8Error) -> Self {
        Self(Kind::FromUtf8Error {
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
            | Kind::FailedToExtractRunInfo { backtrace, .. }
            | Kind::FailedToStartPod { backtrace, .. }
            | Kind::FromUtf8Error { backtrace, .. }
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
