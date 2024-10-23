#![expect(clippy::panic_in_result_fn, reason = "Panics OK in tests.")]

pub mod fixture;
use anyhow::Result;
use fixture::{get_test_item, store_test, ItemType};
use orcapod::store::ItemKey;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_pod_with_file_store() -> Result<()> {
    test_item_store_with_annotation(&ItemType::Pod)
}

fn test_item_store_with_annotation(item_type: &ItemType) -> Result<()> {
    let store_directory = tempdir()?.path().to_string_lossy().to_string();
    {
        let item = get_test_item(item_type)?;
        let mut store = store_test(Some(&tempdir()?.path().to_string_lossy()))?; // new tests can just call store_test(None)?

        let annotation_file = store.make_annotation_path(&item);

        let spec_file = store.make_path(item_type, item.get_hash(), "spec.yaml");

        // Test save
        store.save_item(&item)?;
        assert!(
            spec_file.exists(),
            "Spec file could not be found after item creation"
        );
        assert_eq!(
            fs::read_to_string(&spec_file)?,
            item.to_yaml()?,
            "spec.yaml does not match item.to_yaml, something went wrong during the write"
        );
        assert!(
            annotation_file.exists(),
            "Cannot find annotation file after saving"
        );

        // Test load
        let loaded_item = store.load_item(
            item_type,
            &ItemKey::NameVer(item.get_name().into(), item.get_version().into()),
        )?;

        assert!(
            loaded_item == item,
            "Loaded item does not match the item was saved"
        );

        let loaded_item_by_hash =
            store.load_item(item_type, &ItemKey::Hash(item.get_hash().into()))?;

        assert!(
            loaded_item_by_hash.is_annotation_none(),
            "Annotation should be empty"
        );

        // Test list pod
        // Should only return a result of 1
        assert!(
            store.list_item(item_type)?.len() == 1,
            "List item should be length of 1"
        );

        // Add another pod with a new version
        let mut item_2 = item.clone();
        item_2.set_name("Second Item Test");

        store.save_item(&item_2)?;

        assert!(
            store.list_item(item_type)?.len() == 2,
            "List item should be length of 2"
        );

        // Test delete
        store.delete_item_annotation(item_type, item_2.get_name(), item_2.get_version())?;

        assert!(
            store.list_item(item_type)?.len() == 1,
            "List item should be length of 1"
        );

        // Delete the first pod
        store.delete_item(
            item_type,
            &ItemKey::NameVer(item.get_name().into(), item.get_version().into()),
        )?;

        assert!(
            store.list_item(item_type)?.is_empty(),
            "List item should be empty"
        );

        // Test the case with where delete wipes out all annotation
        store.save_item(&item)?;
        store.save_item(&item_2)?;

        assert!(
            store.list_item(item_type)?.len() == 2,
            "List item should be length of 2"
        );

        // Delete the entire pod which should get rid of annotation
        store.delete_item(
            item_type,
            &ItemKey::NameVer(item.get_name().into(), item.get_version().into()),
        )?;

        assert!(store.list_item(item_type)?.is_empty(), "List item should be empty after deleting the object itself regardless of how many annotations there are");

        // Test the hash version
        // Test the case with where delete wipes out all annotation
        store.save_item(&item)?;
        store.save_item(&item_2)?;

        assert!(
            store.list_item(item_type)?.len() == 2,
            "List item should be length of 2"
        );

        // Delete the entire pod which should get rid of annotation
        store.delete_item(item_type, &ItemKey::Hash(item.get_hash().into()))?;

        assert!(store.list_item(item_type)?.is_empty(), "List item should be empty after deleting the object itself regardless of how many annotations there are");

        assert!(
            !spec_file.exists(),
            "Spec was found when it should have been deleted"
        );
        assert!(
            !annotation_file.exists(),
            "Annotation file was found when it should have been cleaned up"
        );
    };
    assert!(
        !fs::exists(&store_directory)?,
        "Store directory didn't get destory after store object went out of scope"
    );
    Ok(())
}
