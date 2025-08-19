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
