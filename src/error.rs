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
/// Shorthand for a Result that returns an `OrcaError`.
pub type OrcaResult<T> = Result<T, OrcaError>;
/// Possiable errors you may encounter.
#[derive(Debug)]
pub enum OrcaError {
    /// Returned if a file is not expected to exist.
    FileExists(PathBuf),
    /// Returned if a file is expected to have a parent.
    FileHasNoParent(PathBuf),
    /// Returned if an annotation was expected to exist.
    NoAnnotationFound(String, String, String),
    /// Returned if a regular expression was expected to match.
    NoRegexMatch,
    /// Wrapper around `glob::GlobError`
    GlobError(glob::GlobError),
    /// Wrapper around `glob::PatternError`
    GlobPaternError(glob::PatternError),
    /// Wrapper around `regex::Error`
    RegexError(regex::Error),
    /// Wrapper around `serde_yaml::Error`
    SerdeYamlError(serde_yaml::Error),
    /// Wrapper around `io::Error`
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
