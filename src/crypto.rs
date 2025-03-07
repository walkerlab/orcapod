use crate::error::{Kind, OrcaError, Result};
use serde_yaml;
use sha2::{Digest as _, Sha256};
use std::{collections::BTreeMap, fs::File, io::Read, path::Path};

/// Evaluate checksum hash of streamed data i.e. chunked buffers.
///
/// # Errors
///
/// Will return error if unable to read from stream.
pub fn hash_stream(stream: &mut impl Read) -> Result<String> {
    const BUFFER_SIZE: usize = 8 << 10; // 8KB chunks to match with page size typically found
    let mut hash = Sha256::new();

    let mut buffer = [0; BUFFER_SIZE];

    while {
        let read_size = stream.read(&mut buffer)?;
        hash.update(&buffer[..read_size]);
        read_size == BUFFER_SIZE
    } {}

    Ok(format!("{:x}", hash.finalize()))
}

/// Evaluate checksum hash of raw data in memory.
pub fn hash_buffer(buffer: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(buffer.as_ref()))
}

/// Evaluate checksum hash of a file.
///
/// # Errors
///
/// Will return error if unable to access file.
pub fn hash_file(filepath: impl AsRef<Path>) -> Result<String> {
    hash_stream(&mut match File::open(&filepath) {
        Ok(file) => file,
        Err(error) => {
            return Err(OrcaError::from(Kind::IoErrorWithPath {
                error,
                path: filepath.as_ref().into(),
            }))
        }
    })
}

/// Evaluate checksum hash of a folder.
///
/// # Errors
///
/// Will return error if unable to access any child file.
pub fn hash_dir(dirpath: impl AsRef<Path>) -> Result<String> {
    let summary: BTreeMap<String, String> = match dirpath.as_ref().read_dir() {
        Ok(dir) => dir,
        Err(error) => {
            return Err(OrcaError::from(Kind::IoErrorWithPath {
                error,
                path: dirpath.as_ref().into(),
            }))
        }
    }
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
