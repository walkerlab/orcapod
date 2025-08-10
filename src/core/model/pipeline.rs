use crate::uniffi::model::pipeline::Kernel;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct PipelineNode {
    pub name: String,
    pub kernel: Kernel,
}
