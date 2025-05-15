mod fixture;

use fixture::pipeline;
use orcapod::uniffi::error::Result;

#[test]
fn pipeline_creation() -> Result<()> {
    pipeline()?;
    println!("{:?}", pipeline());
    Ok(())
}
