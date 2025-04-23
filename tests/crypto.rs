#![expect(missing_docs, clippy::panic_in_result_fn, reason = "OK in tests.")]

use orcapod::{
    core::crypto::{hash_buffer, hash_dir, hash_file},
    uniffi::error::Result,
};
use std::fs::read;

#[test]
fn consistent_hash() -> Result<()> {
    let filepath = "./tests/extra/data/images/subject.jpeg";
    assert_eq!(
        hash_file(filepath)?,
        hash_buffer(&read(filepath)?),
        "Checksum not consistent."
    );
    Ok(())
}

#[test]
fn complex_hash() -> Result<()> {
    let dirpath = "./tests/extra/data/images";
    assert_eq!(
        hash_dir(dirpath)?,
        "6c96a478ea25e34fab045bc82858a2980b2cfb22db32e83c01349a8e7ed3b42c".to_owned(),
        "Directory checksum didn't match."
    );
    Ok(())
}
