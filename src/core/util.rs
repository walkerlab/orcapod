use crate::uniffi::error::{Result, selector};
use snafu::OptionExt as _;
use std::{any::type_name, collections::HashMap, fmt};
use std::{cell::OnceCell, sync::LazyLock};
use tokio::runtime::{Builder, Runtime};
use uniffi::deps::async_compat::Compat;

#[expect(
    clippy::unwrap_used,
    reason = "Cannot return `None` since `type_name` always returns `&str`."
)]
pub(crate) fn get_type_name<T>() -> String {
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
pub(crate) fn parse_debug_name<T: fmt::Debug>(instance: &T) -> String {
    format!("{instance:?}")
        .split(' ')
        .map(str::to_owned)
        .next()
        .unwrap()
}

pub(crate) fn get<'map, T>(map: &'map HashMap<String, T>, key: &str) -> Result<&'map T> {
    Ok(map.get(key).context(selector::KeyMissing {
        key: key.to_owned(),
    })?)
}

#[expect(
    clippy::expect_used,
    reason = "Should be able to create Tokio runtime."
)]
pub static ASYNC_RUNTIME: LazyLock<Runtime> =
    LazyLock::new(|| Runtime::new().expect("Unable to create Tokio runtime."));

// #[expect(
//     clippy::expect_used,
//     reason = "Should be able to create Tokio runtime."
// )]
// pub static ASYNC_RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
//     let mut builder = Builder::new_multi_thread();
//     builder.enable_all(); // and others
//     let rt = builder.build().expect("Unable to create Tokio runtime.");
//     rt.block_on(Compat::new(async {}));
//     rt
// });

// static ASYNC_RUNTIME: OnceCell<Runtime> = OnceCell::new();

// pub static ASYNC_RUNTIME: &Runtime =
//     OnceCell::new().get_or_init(|| Runtime::new().expect("Unable to create Tokio runtime."));
