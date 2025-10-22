use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};
use uniffi;

use crate::{error::Result, util::get};

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
impl PathInfo {}

/// File or directory options for BLOBs.
#[derive(uniffi::Enum, Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub enum BlobKind {
    /// A single directory.
    Directory,
    /// A single file.
    #[default]
    File,
}

/// Location of BLOB data.
#[derive(uniffi::Record, Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct URI {
    /// Namespace alias.
    pub namespace: String,
    /// Path within namespace.
    pub path: PathBuf,
}

/// BLOB with metadata.
#[derive(uniffi::Record, Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct Blob {
    /// BLOB available options.
    pub kind: BlobKind,
    /// BLOB location.
    pub location: URI,
    /// BLOB contents checksum.
    #[uniffi(default = "")]
    pub checksum: String,
}

impl Blob {
    /// Create a new `Blob`
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
    /// Util function to convert ``PathSet`` to ``PathBuf`` given a namespace lookup table
    ///
    /// # Errors
    /// Will error out if namespace is missing in namespace lookup
    pub fn to_path_buf(&self, namespace_lookup: &HashMap<String, PathBuf>) -> Result<Vec<PathBuf>> {
        match self {
            Self::Unary(blob) => {
                let base_path = get(namespace_lookup, &blob.location.namespace)?;
                Ok(vec![base_path.join(&blob.location.path)])
            }
            Self::Collection(blobs) => {
                let mut paths = Vec::with_capacity(blobs.len());
                for blob in blobs {
                    let base_path = get(namespace_lookup, &blob.location.namespace)?;
                    paths.push(base_path.join(&blob.location.path));
                }
                Ok(paths)
            }
        }
    }
}

/// A complete set of inputs to be provided to a computational unit.
pub type Packet = HashMap<String, PathSet>;
