use sha2::{Digest as _, Sha256};

use crate::error::Result;
use std::{collections::BTreeMap, fs::File, io::Read, path::Path};

/// Size of the output that the crypto function spit out
pub static HASH_SIZE_IN_BYTES: usize = 32;

/// Function to hash data from a `stream`
///
/// # Errors
/// Will error out if failed to fill buffer for some reason
pub fn hash_stream(stream: &mut impl Read) -> Result<String> {
    const BUFFER_SIZE: usize = 8 << 10; // 8KB chunks to match with page size typically found
    let mut hash = Sha256::new();

    let mut buffer: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];

    while {
        let read_size = stream.read(&mut buffer)?;
        hash.update(&buffer[..read_size]);
        read_size == BUFFER_SIZE
    } {}

    Ok(format!("{:x}", hash.finalize()))
}

/// Function to hash data that is already in memory. This is much cleaner and less overhead compare
/// to mapping data in memory into a ``BufReader``
pub fn hash_buffer(buffer: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(buffer.as_ref()))
}

/// Evaluate checksum hash of a file.
///
/// # Errors
///
/// Will return error if unable to access file.
pub fn hash_file(filepath: impl AsRef<Path>) -> Result<String> {
    hash_stream(&mut File::open(filepath)?)
}

/// Evaluate checksum hash of a folder.
///
/// # Errors
///
/// Will return error if unable to access any child file.
pub fn hash_dir(dirpath: impl AsRef<Path>) -> Result<String> {
    let summary: BTreeMap<String, String> = dirpath
        .as_ref()
        .read_dir()?
        .map(|path| {
            let access_path = path?.path();
            Ok((
                access_path
                    .strip_prefix(&dirpath)?
                    .to_string_lossy()
                    .into_owned(),
                if access_path.is_dir() {
                    hash_dir(access_path)?
                } else {
                    hash_file(access_path)?
                },
            ))
        })
        .collect::<Result<_>>()?;

    Ok(hash_buffer(serde_yaml::to_string(&summary)?))
}
