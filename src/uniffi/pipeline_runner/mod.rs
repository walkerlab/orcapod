use crate::uniffi::error::Result;

use super::pipeline::PipelineJob;

/// # Errors:
/// Error out if fail to start the pipeline job
pub trait PipelineRunner {
    /// Starts the given pipeline job.
    ///
    /// # Errors
    /// Returns an error if the pipeline job fails to start.
    fn start(&self, pipeline_job: PipelineJob) -> Result<()>;
}
/// Docker pipeline runner
pub mod runner;
