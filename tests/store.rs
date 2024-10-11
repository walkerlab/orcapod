use orcapod::model::{Annotation, Pod};
use orcapod::store::{LocalFileStore, OrcaStore};
use std::collections::BTreeMap;
use std::error::Error;
use std::path::PathBuf;

// todo model test: require use of Pod::new
// todo model test: to_yaml for Pod
// todo model test: from_yaml for Pod
// todo model test: hash correct for Pod

// todo store test: save_pod (annotation + spec written)
// todo store test: save_pod with annotation that already exists (Err(AnnotationExists))
// todo store test: save_pod with spec that already exists (skipped and logged)
// todo store test: load_pod (instance matches values in annotation + spec)
// todo store test: load_pod with missing annotation (Err(NoAnnotationFound))
// todo store test: load_pod with missing spec (Err(NoSpecFound))
// todo store test: list_pod (displays correct saved pods)
// todo store test: delete_pod (removes annotation leaves spec)
// todo store test: delete_pod (removes annotation, removes spec dir if last ref'ed annotation)
// todo store test: delete_pod (removes annotation, removes spec dir, removes annotation dir if last version)

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
