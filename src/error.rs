use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct NotFound<'a> {
    pub model: &'a str,
    pub name: &'a str,
    pub version: &'a str,
}

impl fmt::Display for NotFound<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "`{}:{}` {} not found.",
            self.name, self.version, self.model
        )
    }
}

impl Error for NotFound<'_> {}
