use crate::uniffi::{
    error::{Result, selector},
    model::PathSet,
};
use snafu::OptionExt as _;
use std::{
    any::type_name,
    collections::HashMap,
    fmt::{self, Debug},
    hash::Hash,
};

#[expect(
    clippy::unwrap_used,
    reason = "Cannot return `None` since `type_name` always returns `&str`."
)]
pub fn get_type_name<T>() -> String {
    type_name::<T>()
        .split("::")
        .map(str::to_owned)
        .last()
        .unwrap()
}

#[expect(
    clippy::unwrap_used,
    reason = "Cannot return `None` since debug format always returns `String`."
)]
pub fn parse_debug_name<T: fmt::Debug>(instance: &T) -> String {
    format!("{instance:?}")
        .split(' ')
        .map(str::to_owned)
        .next()
        .unwrap()
}

pub fn get<'map, K, T>(map: &'map HashMap<K, T>, key: &K) -> Result<&'map T>
where
    K: Hash + Eq + ToOwned<Owned = K> + Debug,
{
    let temp = map.get(key).context(selector::KeyMissing {
        key: format!("{key:?}"),
    })?;
    Ok(temp)
}

pub fn find_missing_keys<'a>(
    input_map: &HashMap<String, PathSet>,
    keys_to_check: impl Iterator<Item = &'a String>,
) -> Vec<String> {
    keys_to_check
        .filter_map(|key| {
            if input_map.contains_key(key) {
                None
            } else {
                Some(key.clone())
            }
        })
        .collect()
}
