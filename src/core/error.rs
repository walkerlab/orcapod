use crate::uniffi::error::{Kind, OrcaError};
use bollard::errors::Error as BollardError;
use dot_parser::ast::PestError;
use glob;
use serde_json;
use serde_yaml;
use std::{
    backtrace::{Backtrace, BacktraceStatus},
    error::Error,
    fmt::{self, Formatter},
    io, path,
    sync::PoisonError,
};
use tokio::task;

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
impl<T> From<PoisonError<T>> for OrcaError {
    fn from(_: PoisonError<T>) -> Self {
        Self {
            kind: Kind::PoisonError {
                source: Box::<dyn Error + Send + Sync>::from("PoisonError"),
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
            | Kind::EmptyResponseWhenLoadingContainerAltImage { backtrace, .. }
            | Kind::GeneratedNamesOverflow { backtrace, .. }
            | Kind::IncompletePacket { backtrace, .. }
            | Kind::InvalidFilepath { backtrace, .. }
            | Kind::InvalidPodResultTerminatedDatetime { backtrace, .. }
            | Kind::KeyMissing { backtrace, .. }
            | Kind::NoAnnotationFound { backtrace, .. }
            | Kind::NoContainerNames { backtrace, .. }
            | Kind::NoFileName { backtrace, .. }
            | Kind::NoMatchingPodRun { backtrace, .. }
            | Kind::NoRemainingServices { backtrace, .. }
            | Kind::NoTagFoundInContainerAltImage { backtrace, .. }
            | Kind::BollardError { backtrace, .. }
            | Kind::ChronoParseError { backtrace, .. }
            | Kind::DOTError { backtrace, .. }
            | Kind::GlobPatternError { backtrace, .. }
            | Kind::IoError { backtrace, .. }
            | Kind::PathPrefixError { backtrace, .. }
            | Kind::PoisonError { backtrace, .. }
            | Kind::SerdeJsonError { backtrace, .. }
            | Kind::SerdeYamlError { backtrace, .. }
            | Kind::TokioTaskJoinError { backtrace, .. } => {
                write!(f, "{}{}", self.kind, format_stack(backtrace.as_ref()))
            }
        }
    }
}
