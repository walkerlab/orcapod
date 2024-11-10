#![expect(clippy::expect_used, reason = "Expect OK in tests.")]
#![expect(clippy::panic_in_result_fn, reason = "Panics OK in tests.")]

pub mod fixture;
use fixture::{add_storage, pod_style, store_test, TestSetup};
use orcapod::{
    error::Result,
    model::{Annotation, Pod},
    store::{filestore::LocalFileStore, ModelID, ModelInfo, Store},
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

fn basic_test<T: TestSetup + Debug>(model: T) -> Result<()>
where
    T::Target: PartialEq<T> + Debug,
{
    let store = store_test(None)?;
    let stored_model = add_storage(model, &store)?;
    let annotation = stored_model
        .model
        .get_annotation()
        .expect("Annotation missing from `pod_style`");
    assert_eq!(
        store.list_pod()?,
        vec![ModelInfo {
            name: annotation.name.clone(),
            version: annotation.version.clone(),
            hash: stored_model.model.get_hash().to_owned()
        }],
        "List didn't match."
    );
    let loaded_model = stored_model.model.load(&store)?;
    assert_eq!(
        loaded_model, stored_model.model,
        "Loaded model doesn't match."
    );
    Ok(())
}

#[test]
fn pod_basic() -> Result<()> {
    basic_test(pod_style()?)
}

#[test]
fn pod_files() -> Result<()> {
    let store_directory = String::from(tempdir()?.path().to_string_lossy());
    {
        let pod_style = pod_style()?;
        let store = store_test(Some(&store_directory))?;
        let annotation = pod_style
            .annotation
            .as_ref()
            .expect("Annotation missing from `pod_style`");
        let annotation_file = store.make_path::<Pod>(
            &pod_style.hash,
            &LocalFileStore::make_annotation_relpath(&annotation.name, &annotation.version),
        );
        let spec_file = store.make_path::<Pod>(&pod_style.hash, LocalFileStore::SPEC_RELPATH);
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
    let store = store_test(None)?;
    assert_eq!(store.list_pod()?, vec![], "Pod list is not empty.");
    Ok(())
}

#[test]
fn pod_load_from_hash() -> Result<()> {
    let store = store_test(None)?;
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
    let store = store_test(None)?;
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
                name: "new-name".to_owned(),
                version: "0.5.0".to_owned(),
                hash: "13d69656d396c272588dd875b2802faee1a56bd985e3c43c7db276a373bc9ddb".to_owned()
            },
            ModelInfo {
                name: "style-transfer".to_owned(),
                version: "0.67.0".to_owned(),
                hash: "13d69656d396c272588dd875b2802faee1a56bd985e3c43c7db276a373bc9ddb".to_owned()
            }
        ],
        "Pod list didn't return 2 expected entries."
    );
    store.delete_annotation::<Pod>("new-name", "0.5.0")?;
    assert_eq!(
        store.list_pod()?,
        vec![ModelInfo {
            name: "style-transfer".to_owned(),
            version: "0.67.0".to_owned(),
            hash: "13d69656d396c272588dd875b2802faee1a56bd985e3c43c7db276a373bc9ddb".to_owned()
        }],
        "Pod list didn't return 1 expected entry."
    );
    assert!(
        store
            .delete_annotation::<Pod>("style-transfer", "0.67.0")
            .expect_err("Unexpectedly succeeded.")
            .is_deleting_last_annotation(),
        "Returned a different OrcaError than one expected for deleting the last annotation."
    );
    Ok(())
}
