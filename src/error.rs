use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct FileHasNoParent {
    pub filepath: String,
}

impl fmt::Display for FileHasNoParent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "File `{}` has no parent.", self.filepath)
    }
}

impl Error for FileHasNoParent {}

#[derive(Debug)]
pub struct NoAnnotationFound {
    pub class: String,
    pub name: String,
    pub version: String,
}

impl fmt::Display for NoAnnotationFound {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "No annotation found for `{}:{}` {}.",
            self.name, self.version, self.class
        )
    }
}

impl Error for NoAnnotationFound {}

#[derive(Debug)]
pub struct NoSpecFound {
    pub class: String,
    pub name: String,
    pub version: String,
}

impl fmt::Display for NoSpecFound {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "No specification found for `{}:{}` {}.",
            self.name, self.version, self.class
        )
    }
}

impl Error for NoSpecFound {}

#[derive(Debug)]
pub struct AnnotationExists {
    pub class: String,
    pub name: String,
    pub version: String,
}

impl fmt::Display for AnnotationExists {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Annotation found for `{}:{}` {}.",
            self.name, self.version, self.class
        )
    }
}

impl Error for AnnotationExists {}
