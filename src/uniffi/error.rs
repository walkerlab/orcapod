use crate::core::error::Kind;
use std::result;
use thiserror::Error;
/// Shorthand for a Result that returns an `OrcaError`.
pub type Result<T> = result::Result<T, OrcaError>;
/// A stable error API interface.
#[derive(Error, Debug)]
pub struct OrcaError {
    /// Type of error returned.
    pub kind: Kind,
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
