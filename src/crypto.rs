use sha2::{Digest as _, Sha256};

use crate::error::{Kind, OrcaError, Result};
use std::{collections::BTreeSet, fs::File, io::Read, path::Path};

/// Size of the output that the crypto function spit out
pub static HASH_SIZE_IN_BYTES: usize = 32;
static BUFFER_READER_CAP: usize = 1 << 13; // 8KB chunks to match with page size typically found

/// Function to hash data from a `stream`
///
/// # Errors
/// Will error out if failed to fill buffer for some reason
pub fn hash_stream(stream: &mut impl Read) -> Result<String> {
    let mut hash = Sha256::new();

    let mut buffer: [u8; BUFFER_READER_CAP] = [0; BUFFER_READER_CAP];

    loop {
        let num_bytes = stream.read(&mut buffer)?;
        if num_bytes > 0 {
            hash.update(&buffer[..num_bytes]);
        } else {
            break;
        }
    }

    Ok(format!("{:x}", hash.finalize()))
}

/// Function to hash data that is already in memory. This is much cleaner and less overhead compare
/// to mapping data in memory into a ``BufReader``
pub fn hash_bytes(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}

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
        hash_stream(&mut File::open(unref_path)?)
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
