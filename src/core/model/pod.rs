use crate::uniffi::model::pod::{Pod, PodJob};
use serde::{Deserialize as _, Deserializer};
use serde_yaml::{self, Value};
use std::{result, sync::Arc};

#[expect(
    clippy::expect_used,
    reason = "Function signature required by serde API."
)]
pub fn deserialize_pod<'de, D>(deserializer: D) -> result::Result<Arc<Pod>, D::Error>
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
pub fn deserialize_pod_job<'de, D>(deserializer: D) -> result::Result<Arc<PodJob>, D::Error>
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
