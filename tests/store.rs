#![allow(clippy::panic_in_result_fn, reason = "Panics OK in tests.")]

mod fixture;
use core::error::Error;
use fixture::{add_pod_storage, pod_style, store_test};
use orcapod::{
    error::FileHasNoParent,
    model::{to_yaml, Pod},
};
use std::{fs, path::Path};
use tempfile::tempdir;

fn is_dir_two_levels_up_empty(file: &Path) -> Result<bool, Box<dyn Error>> {
    Ok(file
        .parent()
        .ok_or(FileHasNoParent {
            path: file.to_path_buf(),
        })?
        .parent()
        .ok_or(FileHasNoParent {
            path: file.to_path_buf(),
        })?
        .read_dir()?
        .next()
        .is_none())
}

#[test]
fn verify_pod_save_and_delete() -> Result<(), Box<dyn Error>> {
    let store_directory = tempdir()?.path().display().to_string();
    {
        let pod_style = pod_style()?;
        let store = store_test(Some(&store_directory))?; // new tests can just call store_test(None)?
        let annotation_file = store.make_annotation_path(
            "pod",
            &pod_style.hash,
            &pod_style.annotation.name,
            &pod_style.annotation.version,
        );
        let spec_file = store.make_spec_path("pod", &pod_style.hash);
        {
            let pod = add_pod_storage(pod_style, &store)?;
            assert!(fs::exists(&spec_file)?);
            assert_eq!(fs::read_to_string(&spec_file)?, to_yaml::<Pod>(&pod)?);
            assert!(fs::exists(&annotation_file)?);
        };
        assert!(!fs::exists(&spec_file)?);
        assert!(!fs::exists(&annotation_file)?);
        assert!(is_dir_two_levels_up_empty(&spec_file)?);
        assert!(is_dir_two_levels_up_empty(&annotation_file)?);
    };
    assert!(!fs::exists(&store_directory)?);
    Ok(())
}
