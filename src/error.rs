use colored::Colorize;
use serde_yaml;
use std::{
    error::Error,
    fmt,
    fmt::{Display, Formatter},
    io,
    path::PathBuf,
};

// (done) todo: get a none when trying to figure struct_name
#[derive(Debug)]
pub struct OutOfBounds {}
impl Error for OutOfBounds {}
impl Display for OutOfBounds {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "Index is out of bounds.")
    }
}

// todo: wrapper around serde_yaml::from_str
#[derive(Debug)]
pub struct DeserializeError {
    pub path: PathBuf,
    pub error: serde_yaml::Error,
}
impl Error for DeserializeError {}
impl Display for DeserializeError {
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

// todo: wrapper around getting none when trying to find parent
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
            self.path.display().to_string().bright_red()
        )
    }
}

// todo: raise error if a file exists before writting
#[derive(Debug)]
pub struct FileAlreadyExists {
    pub path: PathBuf,
}
impl Error for FileAlreadyExists {}
impl Display for FileAlreadyExists {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "{}{}",
            &self.path.to_string_lossy().bright_cyan(),
            " already exists!".bright_red()
        )
    }
}

// todo: wrapper around serde_yaml::to_string
#[derive(Debug)]
pub struct SerializeError {
    pub item_debug_string: String,
    pub error: serde_yaml::Error,
}
impl Error for SerializeError {}
impl Display for SerializeError {
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

// todo: wrapper around fs::read_to_string and fs::write
#[derive(Debug)]
pub struct IOError {
    pub path: PathBuf,
    pub error: io::Error,
}
impl Error for IOError {}
impl Display for IOError {
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

#[derive(Debug)]
pub struct AnnotationExists {
    pub class: String,
    pub name: String,
    pub version: String,
}
impl Error for AnnotationExists {}
impl Display for AnnotationExists {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "Annotation found for `{}:{}` {}.",
            self.name, self.version, self.class
        )
    }
}

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

#[derive(Debug)]
pub struct NoSpecFound {
    pub class: String,
    pub name: String,
    pub version: String,
}
impl Error for NoSpecFound {}
impl Display for NoSpecFound {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "No specification found for `{}:{}` {}.",
            self.name, self.version, self.class
        )
    }
}
