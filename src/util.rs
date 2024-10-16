use core::any::type_name;
use sha2::{Digest, Sha256};

pub fn get_struct_name<T>() -> String {
    type_name::<T>()
        .split("::")
        .map(str::to_string)
        .collect::<Vec<String>>()[2]
        .to_lowercase()
}

pub fn hash(buffer: &str) -> String {
    format!("{:X}", Sha256::digest(buffer))
}
