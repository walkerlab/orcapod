#![allow(clippy::panic_in_result_fn, reason = "Panics OK in tests.")]

mod fixture;
use core::error::Error;
use fixture::{add_pod_storage, pod_style, store_test};
use orcapod::error::FileHasNoParent;
use std::fs;

#[test]
fn verify_pod_save_and_delete() -> Result<(), Box<dyn Error>> {
    let store_directory = "./test_store";
    {
        let pod_style = pod_style()?;
        let store = store_test(store_directory);
        let annotation_file = store.make_annotation_path(
            "pod",
            &pod_style.hash,
            &pod_style.annotation.name,
            &pod_style.annotation.version,
        );
        let spec_file = store.make_spec_path("pod", &pod_style.hash);
        {
            let _pod = add_pod_storage(pod_style, &store)?;
            assert!(fs::exists(&annotation_file)?);
            assert!(fs::exists(&spec_file)?);
        };
        assert!(!fs::exists(&annotation_file)?);
        assert!(!fs::exists(&spec_file)?);
        assert!(annotation_file
            .parent()
            .ok_or(FileHasNoParent {
                path: annotation_file.clone()
            })?
            .parent()
            .ok_or(FileHasNoParent {
                path: annotation_file.clone()
            })?
            .read_dir()?
            .next()
            .is_none());
        assert!(spec_file
            .parent()
            .ok_or(FileHasNoParent {
                path: spec_file.clone()
            })?
            .parent()
            .ok_or(FileHasNoParent {
                path: spec_file.clone()
            })?
            .read_dir()?
            .next()
            .is_none());
    };
    assert!(!fs::exists(store_directory)?);
    Ok(())
}
