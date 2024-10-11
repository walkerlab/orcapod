use sha2::{Digest, Sha256};

pub fn get_struct_name<T>() -> String {
    std::any::type_name::<T>()
        .split("::")
        .collect::<Vec<&str>>()[2]
        .to_lowercase()
}

pub fn hash_buffer(buffer: &str) -> String {
    format!("{:X}", Sha256::digest(buffer))
}

// pretty table could go here
