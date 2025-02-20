use sha2::{Digest as _, Sha256};
use std::any::type_name;

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

pub fn hash(buffer: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(buffer.as_ref()))
}
