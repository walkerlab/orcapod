use crate::{core::error::selector, uniffi::error::Result};
use snafu::OptionExt as _;
use std::{any::type_name, collections::HashMap};

#[expect(
    clippy::unwrap_used,
    reason = "`last()` cannot return `None` since `type_name` always returns `&str`."
)]
pub fn get_type_name<T>() -> String {
    type_name::<T>()
        .split("::")
        .map(str::to_owned)
        .last()
        .unwrap()
}

pub fn get<'map, T>(map: &'map HashMap<String, T>, key: &str) -> Result<&'map T> {
    Ok(map.get(key).context(selector::KeyMissing {
        key: key.to_owned(),
    })?)
}
