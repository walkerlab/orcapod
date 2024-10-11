use orcapod::model::{Annotation, Pod};
use orcapod::store::{LocalFileStore, OrcaStore};
use regex::Regex;
use std::collections::BTreeMap;
use std::error::Error;
use std::path::PathBuf;
use std::time::Instant;

#[test]
fn pod() -> Result<(), Box<dyn Error>> {
    // todo: clean up
    let fs = LocalFileStore {
        location: PathBuf::from("./test_store"),
    };
    let pod = Pod::new(
        Annotation {
            name: String::from("style-transfer"),
            description: String::from("This is an example pod."),
            version: String::from("0.67.0"),
        },
        String::from("tail -f /dev/null"),
        String::from("zenmldocker/zenml-server:0.67.0"),
        BTreeMap::from([
            (
                String::from("painting"),
                PathBuf::from("/input/painting.png"),
            ),
            (String::from("image"), PathBuf::from("/input/image.png")),
        ]),
        PathBuf::from("/output"),
        BTreeMap::from([(String::from("styled"), PathBuf::from("./styled.png"))]),
        0.25,                   // 250 millicores as frac cores
        (2 as u64) * (1 << 30), // 2GiB in bytes
        String::from("https://github.com/zenml-io/zenml/tree/0.67.0"),
    )?;

    fs.save_pod(&pod)?;

    let pod_des = fs.load_pod("style-transfer", "0.67.0")?;
    println!("{:?}", fs.list_pod());
    println!("{:?}", pod_des);

    fs.delete_pod("style-transfer", "0.67.0")?;

    Ok(())
}
