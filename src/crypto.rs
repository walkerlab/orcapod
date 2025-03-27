use crate::{
    error::{Kind, OrcaError, Result},
    model::{Blob, BlobKind, NameSpaceLookup},
};
use serde_yaml;
use sha2::{Digest as _, Sha256};
use std::{collections::HashMap, fs::File, io::Read, path::Path};

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
    if !filepath.as_ref().exists() {
        return Err(OrcaError::from(Kind::InputFileOrFolderNotFound {
            path: filepath.as_ref().to_path_buf(),
        }));
    }
    hash_stream(&mut File::open(&filepath)?)
}

/// Evaluate checksum hash of a folder.
///
/// # Errors
///
/// Will return error if unable to access any child file.
pub fn hash_dir(dirpath: impl AsRef<Path>) -> Result<String> {
    let summary: HashMap<String, String> = dirpath
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

/// Evaluate checksum blob
///
/// # Errors
/// Will return an io error if it fails to hash the file or folder
pub fn compute_checksum_for_blob(blob: Blob, store_map: &NameSpaceLookup) -> Result<Blob> {
    Ok(Blob {
        checksum: match blob.kind {
            BlobKind::File => hash_file(store_map.resolve_path(&blob.path)?)?,
            BlobKind::Directory => hash_dir(store_map.resolve_path(&blob.path)?)?,
        },
        ..blob
    })
}
