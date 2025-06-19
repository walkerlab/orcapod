use crate::{
    core::util::get_type_name,
    uniffi::{
        error::Result,
        model::{Pod, PodJob},
    },
};
use heck::ToSnakeCase as _;
use indexmap::IndexMap;
use serde::{Deserialize as _, Deserializer, Serialize, Serializer};
use serde_yaml::{self, Value};
use std::{
    collections::{BTreeMap, HashMap},
    result,
    sync::Arc,
};
/// Converts a model instance into a consistent yaml.
///
/// # Errors
///
/// Will return `Err` if there is an issue converting an `instance` into YAML (w/o annotation).
pub fn to_yaml<T: Serialize>(instance: &T) -> Result<String> {
    let mapping: IndexMap<String, Value> = serde_yaml::from_str(&serde_yaml::to_string(instance)?)?; // cast to map
    let mut yaml = serde_yaml::to_string(
        &mapping
            .iter()
            .filter_map(|(k, v)| match &**k {
                "annotation" | "hash" => None,
                "pod" | "pod_job" => Some((k, v["hash"].clone())),
                _ => Some((k, v.clone())),
            })
            .collect::<IndexMap<_, _>>(),
    )?; // skip fields and convert refs to hash pointers
    yaml.insert_str(
        0,
        &format!("class: {}\n", get_type_name::<T>().to_snake_case()),
    ); // replace class at top
    Ok(yaml)
}

pub(crate) fn serialize_hashmap<S, K: Ord + Serialize, V: Serialize>(
    map: &HashMap<K, V>,
    serializer: S,
) -> result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let sorted = map.iter().collect::<BTreeMap<_, _>>();
    sorted.serialize(serializer)
}

#[expect(clippy::ref_option, reason = "Serde requires this signature.")]
pub(crate) fn serialize_hashmap_option<S, K: Ord + Serialize, V: Serialize>(
    map_option: &Option<HashMap<K, V>>,
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

#[expect(
    clippy::expect_used,
    reason = "Function signature required by serde API."
)]
pub(crate) fn deserialize_pod<'de, D>(deserializer: D) -> result::Result<Arc<Pod>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    (value).as_str().map_or_else(
        || {
            Ok(serde_yaml::from_value(value.clone())
                .expect("Failed to convert from serde value to specific type."))
        },
        |hash| {
            Ok({
                Pod {
                    hash: hash.to_owned(),
                    ..Pod::default()
                }
                .into()
            })
        },
    )
}

#[expect(
    clippy::expect_used,
    reason = "Function signature required by serde API."
)]
pub(crate) fn deserialize_pod_job<'de, D>(deserializer: D) -> result::Result<Arc<PodJob>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    (value).as_str().map_or_else(
        || {
            Ok(serde_yaml::from_value(value.clone())
                .expect("Failed to convert from serde value to specific type."))
        },
        |hash| {
            Ok({
                PodJob {
                    hash: hash.to_owned(),
                    ..PodJob::default()
                }
                .into()
            })
        },
    )
}
