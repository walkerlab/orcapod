use heck::ToSnakeCase;
use sha2::{Digest, Sha256};
use std::any::type_name;

#[expect(
    clippy::unwrap_used,
    reason = "`last()` cannot return `None` since `type_name` always returns `&str`."
)]
pub fn get_type_name<T>() -> String {
    type_name::<T>()
        .split("::")
        .collect::<Vec<&str>>()
        .last()
        .unwrap()
        .to_snake_case()
}

pub fn hash(buffer: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(buffer.as_ref()))
}
