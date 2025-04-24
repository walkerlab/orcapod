use crate::uniffi::error::{Result, selector};
use snafu::OptionExt as _;
use std::{any::type_name, collections::HashMap, fmt};

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

pub fn get<'map, T>(map: &'map HashMap<String, T>, key: &str) -> Result<&'map T> {
    Ok(map.get(key).context(selector::KeyMissing {
        key: key.to_owned(),
    })?)
}
