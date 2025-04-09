#![expect(
    clippy::field_scoped_visibility_modifiers,
    reason = "Needed since SNAFU dynamically generating selectors."
)]

use bollard::errors::Error as BollardError;
use glob;
use serde_json;
use serde_yaml;
use snafu::prelude::Snafu;
use std::{
    backtrace::Backtrace,
    io,
    path::{self, PathBuf},
    result,
};
/// Shorthand for a Result that returns an `OrcaError`.
pub type Result<T, E = OrcaError> = result::Result<T, E>;
/// Possible errors you may encounter.
#[derive(Snafu, Debug)]
#[snafu(module(selector), visibility(pub(crate)), context(suffix(false)))]
pub(crate) enum Kind {
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
/// A stable error API interface.
#[derive(Snafu)]
pub struct OrcaError(pub(crate) Kind);

impl OrcaError {
    /// Returns `true` if the error was caused by an invalid model annotation.
    pub const fn is_invalid_annotation(&self) -> bool {
        matches!(self.0, Kind::NoAnnotationFound { .. })
    }
    /// Returns `true` if the error was caused by querying a purged pod run.
    pub const fn is_purged_pod_run(&self) -> bool {
        matches!(self.0, Kind::NoMatchingPodRun { .. })
    }
}
