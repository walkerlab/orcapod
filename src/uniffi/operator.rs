use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Operator class that map `input_keys` to `output_key`, effectively renaming it
/// For use in pipelines
#[derive(uniffi::Object, Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct MapOperator {
    /// Mapping of input keys to output keys
    pub map: HashMap<String, String>,
}
