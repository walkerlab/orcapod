use crate::{core::crypto::hash_blob, uniffi::error::Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};
use uniffi;

/// Path sets are named and represent an abstraction for the file(s) that represent some particular
/// data within a compute environment.
#[derive(uniffi::Record, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PathInfo {
    /// Path to file or directory within compute environment.
    pub path: PathBuf,
    /// Expected naming pattern.
    pub match_pattern: String,
}

#[uniffi::export]
impl PathInfo {
    #[uniffi::constructor]
    /// Create a new `PathInfo` with the given path and match pattern.
    pub const fn new(path: PathBuf, match_pattern: String) -> Self {
        Self {
            path,
            match_pattern,
        }
    }
}

/// File or directory options for BLOBs.
#[derive(uniffi::Enum, Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub enum BlobKind {
    /// A single file.
    #[default]
    File,
    /// A single directory.
    Directory,
}

/// Location of BLOB data.
#[derive(uniffi::Record, Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct URI {
    /// Namespace alias.
    pub namespace: String,
    /// Path within namespace.
    pub path: PathBuf,
}

#[uniffi::export]
impl URI {
    #[uniffi::constructor]
    /// Create a new URI with the given namespace and path.
    pub const fn new(namespace: String, path: PathBuf) -> Self {
        Self { namespace, path }
    }
}

/// BLOB with metadata.
#[derive(uniffi::Record, Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct Blob {
    /// BLOB available options.
    pub kind: BlobKind,
    /// BLOB location.
    pub location: URI,
    /// BLOB contents checksum.
    pub checksum: String,
}

#[uniffi::export]
impl Blob {
    /// Create a new BLOB with the given kind, location, and checksum.
    #[uniffi::constructor]
    pub const fn new(kind: BlobKind, location: URI) -> Self {
        Self {
            kind,
            location,
            checksum: String::new(),
        }
    }
}

/// A single BLOB or a collection of BLOBs.
#[derive(uniffi::Enum, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum PathSet {
    /// A single BLOB.
    Unary(Blob),
    /// A series of BLOBs.
    Collection(Vec<Blob>),
}

impl PathSet {
    pub(crate) fn hash_content(&self, namespace_lookup: &HashMap<String, PathBuf>) -> Result<Self> {
        match self {
            Self::Unary(blob) => Ok(Self::Unary(hash_blob(namespace_lookup, blob)?)),
            Self::Collection(blobs) => Ok(Self::Collection(
                blobs
                    .iter()
                    .map(|blob| hash_blob(namespace_lookup, blob))
                    .collect::<Result<_>>()?,
            )),
        }
    }
}

/// A complete set of inputs to be provided to a computational unit.
pub type Packet = HashMap<String, PathSet>;
