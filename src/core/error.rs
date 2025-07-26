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
};
use tokio::sync::oneshot;
use tokio::task;

impl From<BollardError> for OrcaError {
    fn from(error: BollardError) -> Self {
        Self {
            kind: Kind::BollardError {
                source: error,
                backtrace: Some(Backtrace::capture()),
            },
        }
    }
}
impl From<oneshot::error::RecvError> for OrcaError {
    fn from(error: oneshot::error::RecvError) -> Self {
        Self {
            kind: Kind::ChannelReceiveError {
                source: error,
                backtrace: Some(Backtrace::capture()),
            },
        }
    }
}
impl From<glob::PatternError> for OrcaError {
    fn from(error: glob::PatternError) -> Self {
        Self {
            kind: Kind::GlobPatternError {
                source: error,
                backtrace: Some(Backtrace::capture()),
            },
        }
    }
}
impl From<io::Error> for OrcaError {
    fn from(error: io::Error) -> Self {
        Self {
            kind: Kind::IoError {
                source: error,
                backtrace: Some(Backtrace::capture()),
            },
        }
    }
}
impl From<path::StripPrefixError> for OrcaError {
    fn from(error: path::StripPrefixError) -> Self {
        Self {
            kind: Kind::PathPrefixError {
                source: error,
                backtrace: Some(Backtrace::capture()),
            },
        }
    }
}
impl From<serde_json::Error> for OrcaError {
    fn from(error: serde_json::Error) -> Self {
        Self {
            kind: Kind::SerdeJsonError {
                source: error,
                backtrace: Some(Backtrace::capture()),
            },
        }
    }
}
impl From<serde_yaml::Error> for OrcaError {
    fn from(error: serde_yaml::Error) -> Self {
        Self {
            kind: Kind::SerdeYamlError {
                source: error,
                backtrace: Some(Backtrace::capture()),
            },
        }
    }
}
impl From<task::JoinError> for OrcaError {
    fn from(error: task::JoinError) -> Self {
        Self {
            kind: Kind::TokioTaskJoinError {
                source: error,
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
            | Kind::EmptyResponseWhenLoadingContainerAltImage { backtrace, .. }
            | Kind::FailedToParseDot { backtrace, .. }
            | Kind::GeneratedNamesOverflow { backtrace, .. }
            | Kind::InvalidFilepath { backtrace, .. }
            | Kind::InvalidIndex { backtrace, .. }
            | Kind::InvalidPodResultTerminatedDatetime { backtrace, .. }
            | Kind::KeyMissing { backtrace, .. }
            | Kind::NoAnnotationFound { backtrace, .. }
            | Kind::NoContainerNames { backtrace, .. }
            | Kind::NoFileName { backtrace, .. }
            | Kind::NoMatchingPodRun { backtrace, .. }
            | Kind::NoRemainingServices { backtrace, .. }
            | Kind::NoTagFoundInContainerAltImage { backtrace, .. }
            | Kind::PodJobSubmissionFailed { backtrace, .. }
            | Kind::PodJobProcessingError { backtrace, .. }
            | Kind::StatusConversionFailure { backtrace, .. }
            | Kind::UnsupportedPathType { backtrace, .. }
            | Kind::BollardError { backtrace, .. }
            | Kind::ChannelReceiveError { backtrace, .. }
            | Kind::GlobPatternError { backtrace, .. }
            | Kind::IoError { backtrace, .. }
            | Kind::PathPrefixError { backtrace, .. }
            | Kind::SendError { backtrace, .. }
            | Kind::SerdeJsonError { backtrace, .. }
            | Kind::SerdeYamlError { backtrace, .. }
            | Kind::TokioTaskJoinError { backtrace, .. } => {
                write!(f, "{}{}", self.kind, format_stack(backtrace.as_ref()))
            }
        }
    }
}
