use core::any::type_name;
use sha2::{Digest, Sha256};

pub fn get_struct_name<T>() -> String {
    type_name::<T>()
        .split("::")
        .map(str::to_string)
        .collect::<Vec<String>>()
        .last()
        .map_or_else(String::new, |struct_name| struct_name.to_lowercase())
}

pub fn hash(buffer: &str) -> String {
    format!("{:X}", Sha256::digest(buffer))
}
