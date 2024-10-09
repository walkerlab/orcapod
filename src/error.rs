use std::{
    error::Error,
    fmt::{Debug, Display},
    io,
    path::PathBuf,
};

use colored::Colorize;

#[derive(Debug)]
pub struct DeserializeError {
    pub path: PathBuf,
    pub error: serde_yaml::Error,
}

impl Display for DeserializeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

impl Error for DeserializeError {}

#[derive(Debug)]
pub struct FailedToExtractParentFolder {
    pub path: PathBuf,
}

impl Error for FailedToExtractParentFolder {}

impl Display for FailedToExtractParentFolder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}",
            "Unable to extract folder path".bright_red(),
            &self.path.to_string_lossy().bright_cyan(),
        )
    }
}

#[derive(Debug)]
pub struct FileAlreadyExists {
    pub path: PathBuf,
}

impl Error for FileAlreadyExists {}

impl Display for FileAlreadyExists {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}",
            &self.path.to_string_lossy().bright_cyan(),
            " already exists!".bright_red()
        )
    }
}

#[derive(Debug)]
pub struct SerializeError {
    pub item_debug_string: String,
    pub error: serde_yaml::Error,
}

impl Error for SerializeError {}

impl Display for SerializeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

#[derive(Debug)]
pub struct IOError {
    pub path: PathBuf,
    pub error: io::Error,
}

impl Error for IOError {}

impl Display for IOError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
