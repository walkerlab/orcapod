use crate::{
    core::util::get_type_name,
    uniffi::{
        error::Result,
        model::{Pod, PodJob},
    },
};
use heck::ToSnakeCase as _;
use serde::{Deserialize as _, Deserializer, Serialize, Serializer};
use serde_yaml;
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
    let mut yaml = serde_yaml::to_string(instance)?;
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

pub(crate) fn serialize_pod<S>(pod: &Pod, serializer: S) -> result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&pod.hash)
}

pub(crate) fn deserialize_pod<'de, D>(deserializer: D) -> result::Result<Arc<Pod>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Pod {
        hash: String::deserialize(deserializer)?,
        ..Pod::default()
    }
    .into())
}

pub(crate) fn serialize_pod_job<S>(
    pod_job: &PodJob,
    serializer: S,
) -> result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&pod_job.hash)
}

pub(crate) fn deserialize_pod_job<'de, D>(deserializer: D) -> result::Result<Arc<PodJob>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(PodJob {
        hash: String::deserialize(deserializer)?,
        ..PodJob::default()
    }
    .into())
}
