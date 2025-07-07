#![expect(
    clippy::expect_used,
    missing_docs,
    clippy::panic_in_result_fn,
    clippy::indexing_slicing,
    reason = "OK in tests."
)]

pub mod fixture;
use fixture::{
    NAMESPACE_LOOKUP_READ_ONLY, TestDirs, TestSetup, pod_job_style, pod_result_style, pod_style,
};
use orcapod::{
    core::{crypto::hash_buffer, model::to_yaml},
    uniffi::{
        error::Result,
        model::{Annotation, ModelType},
        store::{ModelID, ModelInfo, Store as _, filestore::LocalFileStore},
    },
};
use std::{collections::HashMap, fmt::Debug, ops::Deref as _, path::Path, sync::Arc};

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

fn basic_test<T: TestSetup + PartialEq + Debug>(model: &T, expected_model: &T) -> Result<()> {
    let test_dirs = TestDirs::new(&HashMap::from([("default".to_owned(), None::<String>)]))?;
    let store = LocalFileStore::new(test_dirs.0["default"].path().to_path_buf());
    model.save(&store)?;
    let annotation = model.get_annotation().expect("Annotation missing.");
    assert_eq!(
        model.list(&store)?,
        vec![
            ModelInfo {
                name: Some(annotation.name.clone()),
                version: Some(annotation.version.clone()),
                hash: model.get_hash().to_owned(),
            },
            ModelInfo {
                name: None,
                version: None,
                hash: model.get_hash().to_owned(),
            },
        ],
        "List didn't match."
    );
    assert_eq!(
        &model.load(&store)?,
        expected_model,
        "Loaded model doesn't match."
    );
    model.delete(&store)?;
    assert_eq!(model.list(&store)?, vec![], "Failed to delete model.");
    Ok(())
}

#[test]
fn pod_basic() -> Result<()> {
    let model = pod_style()?;
    basic_test(&model, &model)?;
    Ok(())
}

#[test]
fn pod_job_basic() -> Result<()> {
    let mut expected_model = pod_job_style(&NAMESPACE_LOOKUP_READ_ONLY)?;
    let mut pod = expected_model.pod.deref().clone();
    pod.annotation = None;
    expected_model.pod = Arc::new(pod);
    basic_test(
        &pod_job_style(&NAMESPACE_LOOKUP_READ_ONLY)?,
        &expected_model,
    )?;
    Ok(())
}

#[test]
fn pod_result_basic() -> Result<()> {
    let mut expected_model = pod_result_style(&NAMESPACE_LOOKUP_READ_ONLY)?;
    let mut pod_job = expected_model.pod_job.deref().clone();
    let mut pod = expected_model.pod_job.pod.deref().clone();
    pod_job.annotation = None;
    pod.annotation = None;
    pod_job.pod = Arc::new(pod);
    expected_model.pod_job = Arc::new(pod_job);
    basic_test(
        &pod_result_style(&NAMESPACE_LOOKUP_READ_ONLY)?,
        &expected_model,
    )?;
    Ok(())
}

#[test]
fn pod_files() -> Result<()> {
    let test_dirs = TestDirs::new(&HashMap::from([("default".to_owned(), None::<String>)]))?;
    let store = LocalFileStore::new(test_dirs.0["default"].path().to_path_buf());
    let pod_style = pod_style()?;
    let annotation = pod_style
        .annotation
        .as_ref()
        .expect("Annotation missing from `pod_style`");
    let annotation_file = store.make_path(
        &pod_style,
        &pod_style.hash,
        LocalFileStore::make_annotation_relpath(&annotation.name, &annotation.version),
    );
    let spec_file = store.make_path(&pod_style, &pod_style.hash, LocalFileStore::SPEC_RELPATH);

    store.save_pod(&pod_style)?;
    assert!(spec_file.exists(), "Spec file missing.");
    assert!(annotation_file.exists(), "Annotation file missing.");

    store.delete_pod(&ModelID::Annotation {
        name: annotation.name.clone(),
        version: annotation.version.clone(),
    })?;
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
    let test_dirs = TestDirs::new(&HashMap::from([("default".to_owned(), None::<String>)]))?;
    let store = LocalFileStore::new(test_dirs.0["default"].path().to_path_buf());
    assert_eq!(store.list_pod()?, vec![], "Pod list is not empty.");
    Ok(())
}

#[test]
fn pod_load_from_hash() -> Result<()> {
    let test_dirs = TestDirs::new(&HashMap::from([("default".to_owned(), None::<String>)]))?;
    let store = LocalFileStore::new(test_dirs.0["default"].path().to_path_buf());
    let mut pod = pod_style()?;
    store.save_pod(&pod)?;
    pod.annotation = None;
    assert_eq!(
        store.load_pod(&ModelID::Hash {
            r#ref: pod.hash.clone()
        })?,
        pod,
        "Loaded model from hash doesn't match."
    );
    Ok(())
}

#[test]
fn pod_annotation_delete() -> Result<()> {
    let test_dirs = TestDirs::new(&HashMap::from([("default".to_owned(), None::<String>)]))?;
    let store = LocalFileStore::new(test_dirs.0["default"].path().to_path_buf());
    let mut pod = pod_style()?;
    store.save_pod(&pod)?;
    let model_version = &pod.annotation.as_ref().map(|x| x.version.clone());
    let model_hash = &pod.hash;
    // case 1: save new annotation, assert list gives 3 entries: hash, annotations (original, new).
    pod.annotation = Some(Annotation {
        name: "new-name".to_owned(),
        version: "0.5.0".to_owned(),
        description: String::new(),
    });
    store.save_pod(&pod)?;
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
    // case 2: delete new annotation, assert list gives 2 entries: hash, annotation (original).
    store.delete_annotation(&ModelType::Pod, "new-name", "0.5.0")?;
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
    // case 3: delete original annotation, assert list gives 1 entry: hash.
    store.delete_annotation(
        &ModelType::Pod,
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
    // case 4: delete invalid annotation, error should be returned.
    assert!(
        store
            .delete_annotation(&ModelType::Pod, "style-transfer", "9.9.9")
            .is_err_and(|error| error.is_invalid_annotation() && !format!("{error:?}").is_empty()),
        "Did not raise an invalid annotation error."
    );
    Ok(())
}

#[test]
fn pod_annotation_unique() -> Result<()> {
    let test_dirs = TestDirs::new(&HashMap::from([("default".to_owned(), None::<String>)]))?;
    let store = LocalFileStore::new(test_dirs.0["default"].path().to_path_buf());
    let original_annotation = Annotation {
        name: "example".to_owned(),
        version: "1.0.0".to_owned(),
        description: "original".to_owned(),
    };
    let mut pod = pod_style()?;
    pod.annotation = Some(original_annotation.clone());
    store.save_pod(&pod)?;
    let original_hash = pod.hash.clone();
    // case 1: Only change description, should skip saving model and annotation
    pod.annotation = Some(Annotation {
        description: "new".to_owned(),
        ..original_annotation.clone()
    });
    store.save_pod(&pod)?;
    assert_eq!(
        store.list_pod()?,
        vec![
            ModelInfo {
                name: Some(original_annotation.name.clone()),
                version: Some(original_annotation.version.clone()),
                hash: original_hash.clone(),
            },
            ModelInfo {
                name: None,
                version: None,
                hash: original_hash.clone(),
            },
        ],
        "Pod list didn't return 2 expected entries."
    );
    assert_eq!(
        store
            .load_pod(&ModelID::Annotation {
                name: original_annotation.name.clone(),
                version: original_annotation.version.clone()
            })?
            .annotation,
        Some(original_annotation.clone()),
        "Pod annotation unexpected."
    );
    // case 2: Change description + model, should save model but skip annotation
    pod.output_dir = "/output_2".into();
    pod.hash = hash_buffer(to_yaml(&pod)?);
    let new_hash = pod.hash.clone();
    store.save_pod(&pod)?;
    assert_eq!(
        store.list_pod()?,
        vec![
            ModelInfo {
                name: Some(original_annotation.name.clone()),
                version: Some(original_annotation.version.clone()),
                hash: original_hash.clone(),
            },
            ModelInfo {
                name: None,
                version: None,
                hash: original_hash,
            },
            ModelInfo {
                name: None,
                version: None,
                hash: new_hash,
            },
        ],
        "Pod list didn't return 3 expected entries."
    );
    assert_eq!(
        store
            .load_pod(&ModelID::Annotation {
                name: original_annotation.name.clone(),
                version: original_annotation.version.clone()
            })?
            .annotation,
        Some(original_annotation),
        "Pod annotation unexpected."
    );
    Ok(())
}
