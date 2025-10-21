use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::core::model::ToYaml as _;
use crate::core::{crypto::hash_buffer, model::serialize_hashmap};
use crate::uniffi::error::Result;

/// Operator class that map `input_keys` to `output_key`, effectively renaming it
/// For use in pipelines
#[derive(uniffi::Object, Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct MapOperator {
    /// Unique hash of the map operator
    pub hash: String,
    /// Mapping of input keys to output keys
    #[serde(serialize_with = "serialize_hashmap")]
    pub map: HashMap<String, String>,
}

#[uniffi::export]
impl MapOperator {
    #[uniffi::constructor]
    /// Create a new `MapOperator`
    ///
    /// # Errors
    /// Will error if there are issues converting the map to yaml for hashing
    pub fn new(map: HashMap<String, String>) -> Result<Self> {
        let no_hash = Self {
            map,
            hash: String::new(),
        };

        Ok(Self {
            hash: hash_buffer(no_hash.to_yaml()?),
            ..no_hash
        })
    }
}
