use heck::ToSnakeCase as _;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize, Serializer};
use serde_yaml::{self, Value};
use std::{
    collections::{BTreeMap, HashMap},
    fmt::Debug,
    hash::BuildHasher,
    result,
};

use std::path::PathBuf;

use crate::{error::Result, util::get_type_name};

/// Available models.
#[derive(uniffi::Enum, Debug)]
pub enum ModelType {
    /// See [`Pod`](crate::uniffi::model::pod::Pod).
    Pod,
    /// See [`PodJob`](crate::uniffi::model::pod::PodJob).
    PodJob,
    /// See [`PodResult`](crate::uniffi::model::pod::PodResult).
    PodResult,
    /// See [`Pipeline`](crate::uniffi::model::pipeline::Pipeline).
    Pipeline,
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

/// Trait to handle serialization to yaml for `OrcaPod` models
pub(crate) trait ToYaml: Serialize + Sized + Debug {
    /// Serializes the instance to a YAML string.
    /// # Errors
    /// Will return `Err` if it fail to serialize instance to string
    fn to_yaml(&self) -> Result<String> {
        let mapping: IndexMap<String, Value> = serde_yaml::from_str(&serde_yaml::to_string(self)?)?; // cast to map
        let mut yaml = serde_yaml::to_string(
            &mapping
                .iter()
                .filter_map(|(k, v)| Self::process_field(k, v))
                .collect::<IndexMap<_, _>>(),
        )?; // skip fields and convert refs to hash pointers
        yaml.insert_str(
            0,
            &format!("class: {}\n", get_type_name::<Self>().to_snake_case()),
        ); // replace class at top
        Ok(yaml)
    }

    /// Filter out which field to serialize and which to omit
    ///
    /// # Returns
    /// (`field_name`, `field_value`): to be pass to `to_yaml` for serialization
    /// None: to skip
    fn process_field(field_name: &str, field_value: &Value) -> Option<(String, Value)>;
}

pub(crate) fn serialize_hashmap<S, K: Ord + Serialize, V: Serialize, BH: BuildHasher>(
    map: &HashMap<K, V, BH>,
    serializer: S,
) -> result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let sorted = map.iter().collect::<BTreeMap<_, _>>();
    sorted.serialize(serializer)
}

#[expect(clippy::ref_option, reason = "Serde requires this signature.")]
pub(crate) fn serialize_hashmap_option<S, K: Ord + Serialize, V: Serialize, BH: BuildHasher>(
    map_option: &Option<HashMap<K, V, BH>>,
    serializer: S,
) -> result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let sorted = map_option
        .as_ref()
        .map(|map| map.iter().collect::<BTreeMap<_, _>>());
    sorted.serialize(serializer)
}

/// Utility types for describing packets.
pub mod packet;
/// Models and utility types for pipelines.
pub mod pipeline;
/// Models and utility types for pods.
pub mod pod;
