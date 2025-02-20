use sha2::{Digest as _, Sha256};

use crate::error::Result;
use std::io::{BufRead as _, BufReader, Read};

/// Size of the output that the crypto function spit out
pub static HASH_SIZE_IN_BYTES: usize = 32;

/// Function to hash data from a ``BufReader``
///
/// # Errors
/// Will error out if failed to fill buffer for some reason
pub fn hash_buf_reader<R: Read>(mut reader: BufReader<R>) -> Result<String> {
    let mut hasher = Sha256::new();

    loop {
        let buffer_len = {
            let buffer = reader.fill_buf()?;
            hasher.update(buffer);
            buffer.len()
        };

        if buffer_len == 0 {
            break;
        }
        reader.consume(buffer_len);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// Function to hash data that is already in memory. This is much cleaner and less overhead compare
/// to mapping data in memory into a ``BufReader``
pub fn hash_bytes(data: impl AsRef<[u8]>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}
