use sha2::{Digest as _, Sha256};

use crate::error::{Kind, OrcaError, Result};
use std::{
    collections::BTreeSet,
    fs::File,
    io::{BufRead as _, BufReader, Read},
    path::Path,
};

#[expect(dead_code, reason = "temporary for next pull request")]
/// Size of the output that the crypto function spit out
pub static HASH_SIZE_IN_BYTES: usize = 32;
static BUFFER_READER_CAP: usize = 2 << 13; // 8KB chunks to match with page size typically found

#[expect(dead_code, reason = "temporary for next pull request")]
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

#[expect(dead_code, reason = "temporary for next pull request")]
/// Function to hash data that is already in memory. This is much cleaner and less overhead compare
/// to mapping data in memory into a ``BufReader``
pub fn hash_bytes(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}

#[expect(dead_code, reason = "temporary for next pull request")]
/// Compute the checksum of the content of a folder or file
///
/// # Errors
/// Will error out if accessing the files fails
pub fn compute_checksum_for_path(path: &dyn AsRef<Path>) -> Result<String> {
    let unref_path = path.as_ref();
    if !unref_path.exists() {
        return Err(OrcaError::from(Kind::PathDoesNotExist {
            path: unref_path.to_path_buf(),
        }));
    }

    if unref_path.is_file() {
        // Read and hash in chunks
        let buf_reader = BufReader::with_capacity(BUFFER_READER_CAP, File::open(unref_path)?);

        // Use the buf_reader hashing
        hash_buf_reader(buf_reader)
    } else if unref_path.is_dir() {
        // Path is a directory, thus we will need to recursively hash and sort
        let hashes = unref_path
            .read_dir()?
            .map(|dir_entry| compute_checksum_for_path(&dir_entry?.path()))
            .collect::<Result<BTreeSet<String>>>()?;

        // Combine all the hashes by alpha numeric order
        let mut hashes_buffer = String::new();
        for hash in hashes {
            hashes_buffer.push_str(&hash);
        }

        // Hash the buffer
        Ok(hash_bytes(hashes_buffer))
    } else {
        // Unknown type of path or unsupported, thus panic for now
        Err(OrcaError::from(Kind::UnsupportedPath {
            path: unref_path.to_path_buf(),
        }))
    }
}
