use std::{
    error::Error,
    fmt::{Display, Formatter, Result},
};

#[derive(Debug)]
pub struct FileHasNoParent {
    pub filepath: String,
}
impl Error for FileHasNoParent {}
impl Display for FileHasNoParent {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "File `{}` has no parent.", self.filepath)
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
    fn fmt(&self, f: &mut Formatter) -> Result {
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
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(
            f,
            "No specification found for `{}:{}` {}.",
            self.name, self.version, self.class
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
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(
            f,
            "Annotation found for `{}:{}` {}.",
            self.name, self.version, self.class
        )
    }
}

#[derive(Debug)]
pub struct OutOfBounds {}
impl Error for OutOfBounds {}
impl Display for OutOfBounds {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "Index is out of bounds.")
    }
}
