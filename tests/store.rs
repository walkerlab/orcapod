use orcapod::model::{Annotation, Pod};
use orcapod::store::{LocalFileStore, OrcaStore};
use std::collections::BTreeMap;
use std::error::Error;
use std::path::PathBuf;

#[test]
fn pod() -> Result<(), Box<dyn Error>> {
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

    match fs.save_pod(&pod) {
        Ok(_) => (),
        Err(e) => {
            println!("{}", e)
        }
    };
    println!("{:?}", fs.list_pod());
    // println!("{:?}", fs);
    Ok(())
}
