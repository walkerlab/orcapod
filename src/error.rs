use colored::Colorize;
use std::{
    error::Error,
    fmt,
    fmt::{Display, Formatter},
    io,
    path::PathBuf,
};

/// Wrapper around `serde_yaml::from_str`
#[derive(Debug)]
pub struct DeserializeFailure {
    pub path: PathBuf,
    pub error: serde_yaml::Error,
}
impl Error for DeserializeFailure {}
impl Display for DeserializeFailure {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "{}{}{}{}",
            "Failed to deserialize with error ".bright_red(),
            self.error.to_string().bright_red(),
            " for ".bright_red(),
            self.path.to_string_lossy().bright_cyan()
        )
    }
}

/// Wrapper around getting None when trying to find parent
#[derive(Debug)]
pub struct FileHasNoParent {
    pub path: PathBuf,
}
impl Error for FileHasNoParent {}
impl Display for FileHasNoParent {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "File `{}` has no parent.",
            self.path.to_string_lossy().bright_red()
        )
    }
}

/// Wrapper around `serde_yaml::to_string`
#[derive(Debug)]
pub struct SerializeFailure {
    pub item_debug_string: String,
    pub error: serde_yaml::Error,
}
impl Error for SerializeFailure {}
impl Display for SerializeFailure {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "{}{}{}{}",
            "Failed to seralize ".bright_red(),
            self.item_debug_string.bright_cyan(),
            " with error  ".bright_red(),
            self.error.to_string().bright_red(),
        )
    }
}

/// Wrapper around `fs::read_to_string` and `fs::write`
#[derive(Debug)]
pub struct IOFailure {
    pub path: PathBuf,
    pub error: io::Error,
}
impl Error for IOFailure {}
impl Display for IOFailure {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "{}{}{}{}",
            "IO Error: ".bright_red(),
            &self.error.to_string().bright_red(),
            " at ".bright_red(),
            &self.path.to_string_lossy().cyan(),
        )
    }
}

/// Raise error when file exists but unexpected
#[derive(Debug)]
pub struct FileExists {
    pub path: PathBuf,
}
impl Error for FileExists {}
impl Display for FileExists {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "File `{}` already exists.",
            self.path.to_string_lossy().bright_red()
        )
    }
}

/// Raise error when glob doesn't match on an annotation
#[derive(Debug)]
pub struct NoAnnotationFound {
    pub class: String,
    pub name: String,
    pub version: String,
}
impl Error for NoAnnotationFound {}
impl Display for NoAnnotationFound {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "No annotation found for `{}:{}` {}.",
            self.name, self.version, self.class
        )
    }
}
