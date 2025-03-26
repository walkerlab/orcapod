use std::{any::type_name, collections::HashMap};

use crate::error::{Kind, OrcaError, Result};

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

pub fn get_value_from_map<'map, T>(map: &'map HashMap<String, T>, key: &str) -> Result<&'map T> {
    map.get(key)
        .ok_or(OrcaError::from(Kind::KeyWasNotFoundError {
            key: key.to_owned(),
        }))
}
