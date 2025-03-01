#![expect(missing_docs, clippy::panic_in_result_fn, reason = "OK in tests.")]

use orcapod::{
    crypto::{hash_buffer, hash_dir, hash_file},
    error::Result,
};
use std::fs::read;

#[test]
fn consistent_hash() -> Result<()> {
    let filepath = "./tests/data/images/dog.jpeg";
    assert_eq!(
        hash_file(filepath)?,
        hash_buffer(&read(filepath)?),
        "Checksum not consistent."
    );
    Ok(())
}

#[test]
fn complex_hash() -> Result<()> {
    let dirpath = "./tests/data/images";
    assert_eq!(
        hash_dir(dirpath)?,
        "e53ed7c8d7ddd4337ccc65f2691eeb1a7a21d52f66de293751dd78c33c31d4f6".to_owned(),
        "Directory checksum didn't match."
    );
    Ok(())
}
