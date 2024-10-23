#![expect(clippy::panic_in_result_fn, reason = "Panics OK in tests.")]

pub mod fixture;
use fixture::{add_pod_storage, pod_style, store_test};
use orcapod::{
    error::{OrcaError, OrcaResult},
    model::{to_yaml, Pod},
};
use std::{fs, path::Path};
use tempfile::tempdir;

fn is_dir_two_levels_up_empty(file: &Path) -> OrcaResult<bool> {
    Ok(file
        .parent()
        .ok_or_else(|| OrcaError::FileHasNoParent(file.to_path_buf()))?
        .parent()
        .ok_or_else(|| OrcaError::FileHasNoParent(file.to_path_buf()))?
        .read_dir()?
        .next()
        .is_none())
}

#[test]
fn verify_pod_save_and_delete() -> OrcaResult<()> {
    let store_directory = String::from(tempdir()?.path().to_string_lossy());
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
            assert!(spec_file.exists());
            assert_eq!(fs::read_to_string(&spec_file)?, to_yaml::<Pod>(&pod)?);
            assert!(annotation_file.exists());
        };
        assert!(!spec_file.exists());
        assert!(!annotation_file.exists());
        assert!(is_dir_two_levels_up_empty(&spec_file)?);
        assert!(is_dir_two_levels_up_empty(&annotation_file)?);
    };
    assert!(!fs::exists(&store_directory)?);
    Ok(())
}
