use crate::uniffi::error::{Result, selector};
use snafu::OptionExt as _;
use std::{
    any::type_name,
    borrow::Borrow,
    collections::{BTreeMap, HashMap},
    fmt, hash,
};

#[expect(
    clippy::unwrap_used,
    reason = "Cannot return `None` since debug format always returns `String`."
)]
pub fn get_type_name<T: fmt::Debug>() -> String {
    type_name::<T>()
        .split("::")
        .last()
        .unwrap()
        .to_ascii_lowercase()
}

pub fn get<'map, K, V, Q>(map: &'map HashMap<K, V>, key: &Q) -> Result<&'map V>
where
    Q: ?Sized + hash::Hash + Eq + fmt::Debug,
    K: Borrow<Q> + hash::Hash + Eq,
{
    Ok(map.get(key).context(selector::MissingInfo {
        details: format!("key = {key:?}"),
    })?)
}

pub fn make_key_expr(
    group: &str,
    host: &str,
    topic: &str,
    content: &BTreeMap<String, String>,
) -> String {
    // For each key-value pair in the content, we format it as "key/value" and join them with "/".
    // The final format will be "group/host/topic/key1/value1/key2/value
    let content_converted = content
        .iter()
        .map(|(k, v)| format!("{k}/{v}"))
        .collect::<Vec<String>>()
        .join("/");
    format!("{group}/{host}/{topic}/{content_converted}")
}
