use sha2::{Digest, Sha256};
use std::any::type_name;

#[expect(
    clippy::unwrap_used,
    reason = "`last()` cannot return `None` since `type_name` always returns `&str`."
)]
pub fn get_type_name<T>() -> String {
    type_name::<T>()
        .split("::")
        .map(str::to_string)
        .collect::<Vec<String>>()
        .last()
        .unwrap()
        .to_lowercase()
}

pub fn hash(buffer: &str) -> String {
    format!("{:X}", Sha256::digest(buffer))
}
