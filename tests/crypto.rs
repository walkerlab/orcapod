#![expect(missing_docs, clippy::panic_in_result_fn, reason = "OK in tests.")]

use orcapod::{crypto::compute_checksum_for_path, error::Result};
use std::path::PathBuf;

#[test]
fn compute_checksum_file() -> Result<()> {
    assert_eq!(
        compute_checksum_for_path(&PathBuf::from("./tests/data/images/dog.jpeg"))?,
        "8b44b8ea83b1f5eec3ac16cf941767e629896c465803fb69c21adbbf984516bd",
        "checksum didn't match"
    );
    Ok(())
}

#[test]
fn compute_checksum_folder() -> Result<()> {
    assert_eq!(
        compute_checksum_for_path(&PathBuf::from("./tests/data/images"))?,
        "5a924c9d848e55371d68435058ed6d95bd58ae2e6ca86c0267563dbd57e8f2c6",
        "checksum didn't match"
    );
    Ok(())
}
