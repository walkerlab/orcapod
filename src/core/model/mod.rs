use crate::{core::util::get_type_name, uniffi::error::Result};
use heck::ToSnakeCase as _;
use indexmap::IndexMap;
use serde::{Serialize, Serializer};
use serde_yaml::{self, Value};
use std::{
    collections::{BTreeMap, HashMap},
    hash::BuildHasher,
    result,
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

#[allow(clippy::ref_option, reason = "Serde requires this signature.")]
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

pub(crate) mod pipeline;
pub(crate) mod pod;
