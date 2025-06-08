use crate::uniffi::error::Result;

use super::pipeline::PipelineJob;
use std::fmt;
use std::hash::{Hash, Hasher};

/// # Errors:
/// Error out if fail to start the pipeline job
pub trait PipelineRunner {
    /// Starts the given pipeline job.
    ///
    /// # Errors
    /// Returns an error if the pipeline job fails to start.
    fn start(&self, pipeline_job: PipelineJob) -> Result<()>;
}

#[derive(Debug, Clone)]
/// Struct to store the active pipeline run.
pub struct PipelineRun {
    pipeline_job: PipelineJob,
}

impl fmt::Display for PipelineRun {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PipelineRun {{ pipeline_job: {} }}",
            self.pipeline_job.hash
        )
    }
}

impl PipelineRun {
    /// New function to initialize the pipeline run
    pub const fn new(pipeline_job: PipelineJob) -> Self {
        Self { pipeline_job }
    }
}

impl PartialEq for PipelineRun {
    fn eq(&self, other: &Self) -> bool {
        self.pipeline_job.hash == other.pipeline_job.hash
    }
}

impl Eq for PipelineRun {}

impl Hash for PipelineRun {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.pipeline_job.hash.hash(state);
    }
}

/// Docker pipeline runner
pub mod docker;
