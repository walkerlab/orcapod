use crate::error::OutOfBounds;
use sha2::{Digest, Sha256};
use std::any::type_name;

pub fn get_struct_name<T>() -> Result<String, OutOfBounds> {
    Ok(type_name::<T>()
        .split("::")
        .collect::<Vec<&str>>()
        .get(2)
        .ok_or(OutOfBounds {})?
        .to_string()
        .to_lowercase())
}

pub fn hash_buffer(buffer: &str) -> String {
    format!("{:X}", Sha256::digest(buffer))
}
