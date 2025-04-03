use crate::core::error::Kind;
use snafu::prelude::Snafu;
use std::result;
/// Shorthand for a Result that returns an `OrcaError`.
pub type Result<T, E = OrcaError> = result::Result<T, E>;
/// A stable error API interface.
#[derive(Snafu)]
pub struct OrcaError(pub Kind);

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
