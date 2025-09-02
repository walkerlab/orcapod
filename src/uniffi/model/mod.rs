use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Available models.
#[derive(uniffi::Enum, Debug)]
pub enum ModelType {
    /// See [`Pod`](crate::uniffi::model::pod::Pod).
    Pod,
    /// See [`PodJob`](crate::uniffi::model::pod::PodJob).
    PodJob,
    /// See [`PodResult`](crate::uniffi::model::pod::PodResult).
    PodResult,
}

/// Standard metadata structure for all model instances.
#[derive(uniffi::Record, Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct Annotation {
    /// A unique name.
    pub name: String,
    /// A unique semantic version.
    pub version: String,
    /// A long form description.
    pub description: String,
}

/// Execution requirements for a pod/pipeline, since it doesn't impact the actual reproducibility except for GPU requirement or OOM,
/// it shouldn't be
#[derive(uniffi::Record, Serialize, Deserialize, Debug, PartialEq, Default, Clone)]
pub struct ExecRequirements {
    /// Optimal number of CPU cores needed to run the pod/pipeline provided by the user
    pub recommended_cpus: f32,
    /// Optimal amount of memory needed to run the pod/pipeline provided by the user, code can probably run with less but may hit OOM
    pub recommended_memory: u64,
    /// Optional GPU requirements for the pod/pipeline. If set, then the system should at least meet the architecture requirements.
    pub gpu_requirements: Option<GPURequirement>,
}

/// Specification for GPU requirements in computation.
#[derive(uniffi::Record, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct GPURequirement {
    /// GPU model specification.
    pub model: GPUModel,
    /// Manufacturer recommended memory.
    pub recommended_memory: u64,
    /// Number of GPU cards required.
    pub count: u16,
}

/// GPU model specification.
#[derive(uniffi::Enum, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum GPUModel {
    /// NVIDIA-manufactured card where `String` is the specific CUDA version
    NVIDIA(String),
    /// Any GPU architecture, code is generic enough
    Any,
}

uniffi::custom_type!(PathBuf, String, {
    remote,
    try_lift: |val| Ok(PathBuf::from(&val)),
    lower: |obj| obj.display().to_string(),
});

/// Utility types for describing packets.
pub mod packet;
/// Models and utility types for pipelines.
pub mod pipeline;
/// Models and utility types for pods.
pub mod pod;
