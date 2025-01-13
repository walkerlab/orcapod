#![expect(
    clippy::panic_in_result_fn,
    reason = "Asserts usage panics inside of function"
)]

pub mod fixture;
use std::path::Path;

use anyhow::{Ok, Result};
use fixture::{Model, ModelType, StoreScaffold};
use orcapod::store::{local_store::LocalStore, ModelID};
use tempfile::tempdir;

// Store clean up test
#[test]
fn test_store_wipe() -> Result<()> {
    let temp_dir = tempdir()?.into_path();
    {
        let (_, _) = scaffold_store_with_model(&ModelType::Pod, &temp_dir)?;
    }

    assert!(
        !temp_dir.exists(),
        "Store failed to clean up local file store"
    );
    Ok(())
}

// File Store Tests

// Model Store Tests
// Pod Store Test
#[test]
fn list_pod() -> Result<()> {
    list_model(&ModelType::Pod)
}

#[test]
fn load_pod() -> Result<()> {
    load_model(&ModelType::Pod)
}

#[test]
fn delete_pod_by_hash() -> Result<()> {
    delete_model_by_hash(&ModelType::Pod)
}

#[test]
fn delete_pod_by_annotation() -> Result<()> {
    delete_model_by_annotation(&ModelType::Pod)
}

#[test]
fn delete_pod_annotation() -> Result<()> {
    delete_annotation(&ModelType::Pod)
}

// Pod Job Store Tests
#[test]
fn list_pod_job() -> Result<()> {
    list_model(&ModelType::PodJob)
}

#[test]
fn load_pod_job() -> Result<()> {
    load_model(&ModelType::PodJob)
}

#[test]
fn delete_pod_job_by_hash() -> Result<()> {
    delete_model_by_hash(&ModelType::PodJob)
}

#[test]
fn delete_pod_job_by_annotation() -> Result<()> {
    delete_model_by_annotation(&ModelType::PodJob)
}

#[test]
fn delete_pod_job_annotation() -> Result<()> {
    delete_annotation(&ModelType::PodJob)
}

// Pod Result test
#[test]
fn list_pod_result() -> Result<()> {
    list_model(&ModelType::PodResult)
}

#[test]
fn load_pod_result() -> Result<()> {
    load_model(&ModelType::PodResult)
}

#[test]
fn delete_pod_result() -> Result<()> {
    delete_model_by_hash(&ModelType::PodResult)
}

#[test]
fn load_store_pointer() -> Result<()> {
    // Special case where annotation cannot be None
    let temp_dir = tempdir()?.into_path();
    let (model, store) = scaffold_store_with_model(&ModelType::StorePointer, temp_dir)?;

    assert!(
        model
            == store.load_model(
                &ModelID::Annotation(
                    model.get_annotation().name.clone(),
                    model.get_annotation().version.clone()
                ),
                &ModelType::StorePointer,
            )?
    );

    Ok(())
}

#[test]
fn list_store_pointer() -> Result<()> {
    list_model(&ModelType::StorePointer)
}

#[test]
fn delete_store_pointer_by_hash() -> Result<()> {
    delete_model_by_hash(&ModelType::StorePointer)
}

#[test]
fn delete_store_pointer_by_annotation() -> Result<()> {
    delete_model_by_annotation(&ModelType::StorePointer)
}

fn scaffold_store_with_model(
    model_type: &ModelType,
    store_dir: impl AsRef<Path>,
) -> Result<(Model, StoreScaffold<LocalStore>)> {
    // Upon going out of scope, the scaffold should wipe the directory it used for local file store
    let store = StoreScaffold {
        store: LocalStore::new(store_dir),
    };

    // Test saving
    let mut model = model_type.get_model(&store)?;
    store.save_model(&mut model)?;
    Ok((model, store))
}

fn list_model(model_type: &ModelType) -> Result<()> {
    let temp_dir = tempdir()?.into_path();
    let (model, store) = scaffold_store_with_model(model_type, temp_dir)?;

    let list_result = store.list_model(model_type)?;
    assert!(list_result.len() == 1, "List did not return 1 value");
    assert!(
        list_result[0].hash == model.get_hash(),
        "Model hash didn't match what was put in"
    );

    // If it is type Pod Result, skip the annotation check
    match model_type {
        ModelType::PodResult => (),
        ModelType::Pod | ModelType::PodJob | ModelType::StorePointer => {
            assert!(
                list_result[0].name == model.get_annotation().name,
                "Model name didn't match what was put in"
            );
            assert!(
                list_result[0].version == model.get_annotation().version,
                "Model version didn't match what was put in"
            );
        }
    };

    Ok(())
}

fn load_model(model_type: &ModelType) -> Result<()> {
    let temp_dir = tempdir()?.into_path();
    let (mut model, store) = scaffold_store_with_model(model_type, temp_dir)?;
    model.set_sub_models_annotation_to_none()?;

    match model_type {
        ModelType::PodResult => (),
        ModelType::Pod | ModelType::PodJob | ModelType::StorePointer => {
            // By name and version
            assert!(
                model
                    == store.load_model(
                        &ModelID::Annotation(
                            model.get_annotation().name.clone(),
                            model.get_annotation().version.clone()
                        ),
                        model_type
                    )?,
                "model loaded from store didn't match what was put in"
            );

            // Test Load by hash
            // Set annotation to None
            model.set_annotation(None);
        }
    }

    assert!(
        model == store.load_model(&ModelID::Hash(model.get_hash().to_owned()), model_type)?,
        "model loaded from store didn't match what was put in"
    );

    Ok(())
}

fn delete_annotation(model_type: &ModelType) -> Result<()> {
    let temp_dir = tempdir()?.into_path();
    let (model, store) = scaffold_store_with_model(model_type, temp_dir)?;

    store.delete_item_annotation(
        &model.get_annotation().name,
        &model.get_annotation().version,
        model_type,
    )?;

    let list_result = store.list_model(model_type)?;
    assert!(
        list_result.len() == 1,
        "List result return more than 1 value"
    );
    assert!(
        list_result[0].hash == model.get_hash(),
        "Model hash didn't match what was put in"
    );
    assert!(
        list_result[0].name == String::new(),
        "Model name didn't match what was put in"
    );
    assert!(
        list_result[0].version == String::new(),
        "Model version didn't match what was put in"
    );

    Ok(())
}

fn delete_model_by_hash(model_type: &ModelType) -> Result<()> {
    let temp_dir = tempdir()?.into_path();
    let (model, store) = scaffold_store_with_model(model_type, temp_dir)?;

    store.delete_item(&ModelID::Hash(model.get_hash().to_owned()), model_type)?;
    assert!(
        store.list_model(model_type)?.is_empty(),
        "List is not empty after requesting delete"
    );
    Ok(())
}

fn delete_model_by_annotation(model_type: &ModelType) -> Result<()> {
    let temp_dir = tempdir()?.into_path();
    let (model, store) = scaffold_store_with_model(model_type, temp_dir)?;

    store.delete_item(
        &ModelID::Annotation(
            model.get_annotation().name.clone(),
            model.get_annotation().version.clone(),
        ),
        model_type,
    )?;
    assert!(
        store.list_model(model_type)?.is_empty(),
        "List is not empty after requesting delete"
    );
    Ok(())
}
