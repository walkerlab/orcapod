use sha2::{Digest, Sha256};
use std::any::type_name;

pub fn get_struct_name<T>() -> String {
    type_name::<T>()
        .split("::")
        .collect::<Vec<&str>>()
        .get(2)
        .unwrap()
        .to_string()
        .to_lowercase()
}

pub fn hash(buffer: &str) -> String {
    format!("{:X}", Sha256::digest(buffer))
}
