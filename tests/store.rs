#![expect(clippy::expect_used, reason = "Expect OK in tests.")]
#![expect(clippy::panic_in_result_fn, reason = "Panics OK in tests.")]
#![expect(clippy::unwrap_used, reason = "Expect unwrap to be used in tests.")]
#![expect(clippy::panic, reason = "Expect panic to be used in tests.")]

pub mod fixture;
use fixture::{add_storage, pod_job_style, pod_style, store_fixture, TestSetup, TestStore};
use orcapod::{
    error::Result,
    model::{
        Annotation,
        Input::{Collection, Unary},
        Pod, StorePointer,
    },
    store::{
        filestore::{LocalFileStore, SPEC_RELPATH},
        DataStore, ModelID, ModelInfo, ModelStore,
    },
};
use std::{fmt::Debug, fs, path::Path};
use tempfile::tempdir;

fn is_dir_empty(file: &Path, levels_up: usize) -> Option<bool> {
    Some(
        file.ancestors()
            .nth(levels_up)?
            .read_dir()
            .ok()?
            .next()
            .is_none(),
    )
}

fn basic_test<T: TestSetup + Debug + Clone>(model: T, store: &TestStore) -> Result<(T::Target, T)>
where
    T::Target: PartialEq<T> + Debug,
{
    let stored_model = add_storage(model, store)?;
    let annotation = stored_model
        .model
        .get_annotation()
        .expect("Annotation missing from `pod_style`");
    assert_eq!(
        stored_model.model.list(store)?,
        vec![
            ModelInfo {
                name: Some(annotation.name.clone()),
                version: Some(annotation.version.clone()),
                hash: stored_model.model.get_hash().to_owned(),
            },
            ModelInfo {
                name: None,
                version: None,
                hash: stored_model.model.get_hash().to_owned(),
            },
        ],
        "List didn't match."
    );
    Ok((stored_model.model.load(store)?, stored_model.model.clone()))
}

#[test]
fn pod_basic() -> Result<()> {
    let (loaded_model, stored_model) = basic_test(pod_style()?, &store_fixture(None, false)?)?;
    assert_eq!(loaded_model, stored_model, "Loaded model doesn't match.");
    Ok(())
}

#[test]
fn pod_job_basic() -> Result<()> {
    let (loaded_model, mut stored_model) =
        basic_test(pod_job_style()?, &store_fixture(None, true)?)?;
    stored_model.pod.annotation = None;
    assert_eq!(loaded_model, stored_model, "Loaded model doesn't match.");
    Ok(())
}

#[test]
fn pod_files() -> Result<()> {
    let store_directory = String::from(tempdir()?.path().to_string_lossy());
    {
        let pod_style = pod_style()?;
        let store = store_fixture(Some(&store_directory), false)?;
        let annotation = pod_style
            .annotation
            .as_ref()
            .expect("Annotation missing from `pod_style`");
        let annotation_file = store.make_path::<Pod>(
            &pod_style.hash,
            &LocalFileStore::make_annotation_relpath(&annotation.name, &annotation.version),
        );
        let spec_file = store.make_path::<Pod>(&pod_style.hash, SPEC_RELPATH);
        {
            let _pod = add_storage(pod_style, &store)?;
            assert!(spec_file.exists(), "Spec file missing.");
            assert!(annotation_file.exists(), "Annotation file missing.");
        };
        assert!(!spec_file.exists(), "Spec file wasn't cleaned up.");
        assert!(
            !annotation_file.exists(),
            "Annotation file wasn't cleaned up."
        );
        assert_eq!(
            is_dir_empty(&spec_file, 2),
            Some(true),
            "Model directory wasn't cleaned up."
        );
    };
    assert!(
        !fs::exists(&store_directory)?,
        "Store directory wasn't cleaned up."
    );
    Ok(())
}

#[test]
fn pod_list_empty() -> Result<()> {
    let store = store_fixture(None, false)?;
    assert_eq!(store.list_pod()?, vec![], "Pod list is not empty.");
    Ok(())
}

#[test]
fn pod_load_from_hash() -> Result<()> {
    let store = store_fixture(None, false)?;
    let mut stored_model = add_storage(pod_style()?, &store)?;
    stored_model.model.annotation = None;
    let loaded_pod = stored_model
        .store
        .load_pod(&ModelID::Hash(stored_model.model.hash.clone()))?;
    assert_eq!(
        loaded_pod, stored_model.model,
        "Loaded model from hash doesn't match."
    );
    Ok(())
}

#[test]
fn pod_annotation_delete() -> Result<()> {
    let store = store_fixture(None, false)?;
    let mut stored_model = add_storage(pod_style()?, &store)?;
    stored_model.model.annotation = Some(Annotation {
        name: "new-name".to_owned(),
        version: "0.5.0".to_owned(),
        description: String::new(),
    });
    store.save_pod(&stored_model.model)?;
    assert_eq!(
        store.list_pod()?,
        vec![
            ModelInfo {
                name: Some("new-name".to_owned()),
                version: Some("0.5.0".to_owned()),
                hash: "61d893c39c059b3f6d5e6490edbff1ec2118404ace5031f0b1f5da8a06861085".to_owned(),
            },
            ModelInfo {
                name: Some("style-transfer".to_owned()),
                version: Some("0.67.0".to_owned()),
                hash: "61d893c39c059b3f6d5e6490edbff1ec2118404ace5031f0b1f5da8a06861085".to_owned(),
            },
            ModelInfo {
                name: None,
                version: None,
                hash: "61d893c39c059b3f6d5e6490edbff1ec2118404ace5031f0b1f5da8a06861085".to_owned(),
            },
        ],
        "Pod list didn't return 3 expected entries."
    );
    store.delete_annotation::<Pod>("new-name", "0.5.0")?;
    assert_eq!(
        store.list_pod()?,
        vec![
            ModelInfo {
                name: Some("style-transfer".to_owned()),
                version: Some("0.67.0".to_owned()),
                hash: "61d893c39c059b3f6d5e6490edbff1ec2118404ace5031f0b1f5da8a06861085".to_owned(),
            },
            ModelInfo {
                name: None,
                version: None,
                hash: "61d893c39c059b3f6d5e6490edbff1ec2118404ace5031f0b1f5da8a06861085".to_owned(),
            },
        ],
        "Pod list didn't return 2 expected entry."
    );
    store.delete_annotation::<Pod>("style-transfer", "0.67.0")?;
    assert_eq!(
        store.list_pod()?,
        vec![ModelInfo {
            name: None,
            version: None,
            hash: "61d893c39c059b3f6d5e6490edbff1ec2118404ace5031f0b1f5da8a06861085".to_owned()
        }],
        "Pod list didn't return 1 expected entry."
    );
    assert!(
        store
            .delete_annotation::<Pod>("style-transfer", "9.9.9")
            .expect_err("Unexpectedly succeeded.")
            .is_invalid_annotation(),
        "Returned a different OrcaError than one expected when deleting an invalid annotation."
    );
    Ok(())
}

#[test]
fn test_store_pointer() -> Result<()> {
    // Create store with data since we will be using pod job
    let store = store_fixture(None, true)?;

    // Save the store using store pointer
    store.save_store_pointer(&StorePointer::new(
        Annotation {
            name: "Test Store Pointer".to_owned(),
            version: "0.0.0".to_owned(),
            description: "Example store pointer usage".to_owned(),
        },
        store.get_uri(),
    )?)?;

    // Get the pod job style and modify the image.png to be fetch via store pointer
    // Though in reality, both the default case and the store pointer points to the same location
    let mut pod_job = pod_job_style()?;

    match &mut pod_job.input_stream_mapping.get_mut("image").unwrap() {
        Unary(blob) => {
            blob.store_pointer_name = Some("Test Store Pointer".to_owned());
        }
        Collection(_) => {
            panic!("Shouldn't have a collection as that is not tested yet");
        }
    };

    let (loaded_model, mut stored_model) = basic_test(pod_job, &store)?;
    stored_model.pod.annotation = None;
    assert_eq!(loaded_model, stored_model, "Loaded model doesn't match.");
    Ok(())
}
