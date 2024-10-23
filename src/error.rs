use glob;
use regex;
use serde_yaml;
use std::{
    error::Error,
    fmt,
    fmt::{Display, Formatter},
    io,
    path::PathBuf,
};

pub type OrcaResult<T> = Result<T, OrcaError>;

#[derive(Debug)]
pub enum OrcaError {
    FileExists(PathBuf),
    FileHasNoParent(PathBuf),
    NoAnnotationFound(String, String, String),
    NoRegexMatch,
    GlobError(glob::GlobError),
    GlobPaternError(glob::PatternError),
    RegexError(regex::Error),
    SerdeYamlError(serde_yaml::Error),
    IoError(io::Error),
}
impl Error for OrcaError {}
impl Display for OrcaError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::FileExists(path) => {
                write!(f, "File `{}` already exists.", path.to_string_lossy())
            }
            Self::FileHasNoParent(path) => {
                write!(f, "File `{}` has no parent.", path.to_string_lossy())
            }
            Self::NoAnnotationFound(class, name, version) => {
                write!(f, "No annotation found for `{name}:{version}` {class}.")
            }
            Self::NoRegexMatch => {
                write!(f, "No match for regex.")
            }
            Self::GlobError(error) => write!(f, "{error}"),
            Self::GlobPaternError(error) => write!(f, "{error}"),
            Self::SerdeYamlError(error) => write!(f, "{error}"),
            Self::RegexError(error) => write!(f, "{error}"),
            Self::IoError(error) => write!(f, "{error}"),
        }
    }
}
impl From<glob::GlobError> for OrcaError {
    fn from(error: glob::GlobError) -> Self {
        Self::GlobError(error)
    }
}
impl From<glob::PatternError> for OrcaError {
    fn from(error: glob::PatternError) -> Self {
        Self::GlobPaternError(error)
    }
}
impl From<serde_yaml::Error> for OrcaError {
    fn from(error: serde_yaml::Error) -> Self {
        Self::SerdeYamlError(error)
    }
}
impl From<regex::Error> for OrcaError {
    fn from(error: regex::Error) -> Self {
        Self::RegexError(error)
    }
}
impl From<io::Error> for OrcaError {
    fn from(error: io::Error) -> Self {
        Self::IoError(error)
    }
}
