use crate::{
    core::util::get,
    uniffi::{
        error::{Result, selector},
        model::{Blob, BlobKind},
    },
};
use serde_yaml;
use sha2::{Digest as _, Sha256};
use snafu::ResultExt as _;
use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    hash::RandomState,
    io::Read,
    path::{Path, PathBuf},
};
/// Evaluate checksum hash of streamed data i.e. chunked buffers.
///
/// # Errors
///
/// Will return error if unable to read from stream.
#[expect(
    clippy::indexing_slicing,
    reason = "Reading less than 0 is impossible."
)]
pub(crate) fn hash_stream(stream: &mut impl Read) -> Result<String> {
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
    hash_stream(
        &mut File::open(&filepath).context(selector::InvalidFileOrDirPath {
            path: filepath.as_ref(),
        })?,
    )
}
/// Evaluate checksum hash of a directory.
///
/// # Errors
///
/// Will return error if unable to access any child file.
pub fn hash_dir(dirpath: impl AsRef<Path>) -> Result<String> {
    let summary: BTreeMap<String, String> = dirpath
        .as_ref()
        .read_dir()
        .context(selector::InvalidFileOrDirPath {
            path: dirpath.as_ref(),
        })?
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
/// Evaluate checksum hash of a blob.
///
/// # Errors
///
/// Will return error if hashing fails on file or directory.
pub(crate) fn hash_blob(
    namespace_lookup: &HashMap<String, PathBuf, RandomState>,
    blob: Blob,
) -> Result<Blob> {
    let blob_path = get(namespace_lookup, &blob.location.namespace)?.join(&blob.location.path);
    Ok(Blob {
        checksum: match blob.kind {
            BlobKind::File => hash_file(blob_path)?,
            BlobKind::Directory => hash_dir(blob_path)?,
        },
        ..blob
    })
}
