#![expect(
    clippy::expect_used,
    missing_docs,
    clippy::panic_in_result_fn,
    clippy::unwrap_used,
    reason = "OK in tests."
)]

pub mod fixture;
use fixture::{
    add_storage, pod_job_style, pod_result_style, pod_style, store_temp, TestSetup, TestStore,
    NAMESPACE_LOOKUP_READ_ONLY,
};
use orcapod::{
    crypto::hash_buffer,
    error::Result,
    model::{to_yaml, Annotation, Pod},
    store::{filestore::LocalFileStore, ModelID, ModelInfo, ModelStore as _},
};
use std::{
    fmt::Debug,
    path::{Path, PathBuf},
};

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

fn basic_test<T>(model: T, store: &TestStore) -> Result<(T::Target, T)>
where
    T: TestSetup + Debug + Clone,
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
    let store = store_temp(None, false)?;
    let (loaded_model, stored_model) = basic_test(pod_style()?, &store)?;
    assert_eq!(loaded_model, stored_model, "Loaded model doesn't match.");
    Ok(())
}

#[test]
fn pod_job_basic() -> Result<()> {
    let store = store_temp(None, false)?;
    let (loaded_model, mut stored_model) =
        basic_test(pod_job_style(&NAMESPACE_LOOKUP_READ_ONLY)?, &store)?;
    stored_model.pod.annotation = None;
    assert_eq!(loaded_model, stored_model, "Loaded model doesn't match.");
    Ok(())
}

#[test]
fn pod_result_basic() -> Result<()> {
    let store = store_temp(None, false)?;
    let (loaded_model, mut stored_model) =
        basic_test(pod_result_style(&NAMESPACE_LOOKUP_READ_ONLY)?, &store)?;
    stored_model.pod_job.annotation = None;
    stored_model.pod_job.pod.annotation = None;
    assert_eq!(loaded_model, stored_model, "Loaded model doesn't match.");
    Ok(())
}

#[test]
fn pod_files() -> Result<()> {
    let pod_style = pod_style()?;
    let store = store_temp(None, false)?;
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

    Ok(())
}

#[test]
fn pod_list_empty() -> Result<()> {
    let store = store_temp(None, false)?;
    assert_eq!(store.list_pod()?, vec![], "Pod list is not empty.");
    Ok(())
}

#[test]
fn pod_load_from_hash() -> Result<()> {
    let store = store_temp(None, false)?;
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
    let store = store_temp(None, false)?;
    let mut stored_model = add_storage(pod_style()?, &store)?;
    let model_version = &stored_model
        .model
        .annotation
        .as_ref()
        .map(|x| x.version.clone());
    let model_hash = &stored_model.model.hash;
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
                hash: model_hash.to_owned(),
            },
            ModelInfo {
                name: Some("style-transfer".to_owned()),
                version: model_version.to_owned(),
                hash: model_hash.to_owned(),
            },
            ModelInfo {
                name: None,
                version: None,
                hash: model_hash.to_owned(),
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
                version: model_version.to_owned(),
                hash: model_hash.to_owned(),
            },
            ModelInfo {
                name: None,
                version: None,
                hash: model_hash.to_owned(),
            },
        ],
        "Pod list didn't return 2 expected entry."
    );
    store.delete_annotation::<Pod>(
        "style-transfer",
        &model_version
            .to_owned()
            .expect("Version missing from `pod_style`"),
    )?;
    assert_eq!(
        store.list_pod()?,
        vec![ModelInfo {
            name: None,
            version: None,
            hash: model_hash.to_owned()
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
fn annotation_test() -> Result<()> {
    // TODO later, find a library to read the stdout and assert it.

    // Annotation behavior test, for now there is no std_out asserts since haven't found a library that does it correctly.
    // Case 1: Saving a new model that has no matching annotation for that given model type:
    //      Save everything as normal
    // Case 2: Saving a model but new annotation:
    //      Skip saving spec, but save new annotation
    // Case 3: Saving a model that has a matching annotation already on file, but hash matches for the model.spec
    //      Skip the spec, and skip saving annotation and print a warning
    // Case 4: Saving a model that has a matching annotation, but the hash is different
    //      Save the new spec, and skip saving annotation and print a warning

    let mut pod = pod_style()?;
    let store = store_temp(None, false)?;

    // Case 1
    store.save_pod(&pod)?;

    assert!(make_annotation_path(&store, &pod).exists());
    assert!(make_path(&store, &pod).exists());

    // Case 2
    let old_annotation_version = pod.annotation.as_ref().unwrap().version.clone();
    pod.annotation.as_mut().unwrap().version = "2.0.0".into();

    store.save_pod(&pod)?;

    assert!(make_annotation_path(&store, &pod).exists());
    pod.annotation.as_mut().unwrap().version = old_annotation_version;

    // Case 3
    store.save_pod(&pod)?;

    // Case 4
    pod.output_dir = "output_2".into();
    pod.hash = hash_buffer(to_yaml(&pod)?);

    store.save_pod(&pod)?;

    assert!(make_path(&store, &pod).exists());

    Ok(())
}

fn make_annotation_path(store: &LocalFileStore, pod: &Pod) -> PathBuf {
    store.make_path::<Pod>(
        &pod.hash,
        LocalFileStore::make_annotation_relpath(
            &pod.annotation.as_ref().unwrap().name,
            &pod.annotation.as_ref().unwrap().version,
        ),
    )
}

fn make_path(store: &LocalFileStore, pod: &Pod) -> PathBuf {
    store.make_path::<Pod>(&pod.hash, LocalFileStore::SPEC_RELPATH)
}
