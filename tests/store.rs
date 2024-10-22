#![expect(clippy::panic_in_result_fn, reason = "Panics OK in tests.")]

pub mod fixture;
use anyhow::Result;
use fixture::{add_pod_storage, pod_style, store_test};
use orcapod::{
    error::FileHasNoParent,
    model::{to_yaml, Pod},
};
use std::{fs, path::Path};
use tempfile::tempdir;

fn is_dir_two_levels_up_empty(file: &Path) -> Result<bool> {
    Ok(file
        .parent()
        .ok_or_else(|| FileHasNoParent {
            path: file.to_path_buf(),
        })?
        .parent()
        .ok_or_else(|| FileHasNoParent {
            path: file.to_path_buf(),
        })?
        .read_dir()?
        .next()
        .is_none())
}

#[test]
#[expect(clippy::unwrap_used)]
fn verify_pod_save_and_delete() -> Result<()> {
    let store_directory = String::from(tempdir()?.path().to_string_lossy());
    {
        let pod_style = pod_style()?;
        let mut store = store_test(Some(&store_directory))?; // new tests can just call store_test(None)?
        let annotation_file = store.make_anno_path::<Pod>(
            &pod_style.hash,
            &pod_style.annotation.as_ref().unwrap().name,
            &pod_style.annotation.as_ref().unwrap().version,
        );
        let spec_file = store.make_path::<Pod>(&pod_style.hash, "spec.yaml");
        {
            let pod = add_pod_storage(pod_style, &mut store)?;
            assert!(spec_file.exists());
            assert_eq!(fs::read_to_string(&spec_file)?, to_yaml::<Pod>(&pod)?);
            assert!(annotation_file.exists());
        };
        assert!(!spec_file.exists());
        assert!(!annotation_file.exists());
    };
    assert!(!fs::exists(&store_directory)?);
    Ok(())
}
