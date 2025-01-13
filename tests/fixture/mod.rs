#![expect(clippy::expect_used, reason = "Expect OK in tests.")]
#![expect(clippy::unwrap_used, reason = "Expect test to unwrap without failed")]
#![expect(
    clippy::missing_errors_doc,
    reason = "Integration tests won't be included in documentation."
)]
#![expect(
    clippy::panic,
    reason = "Expect test to panic if incorrect usage for certain situation due to the annotation design"
)]

use anyhow::{anyhow, Result};
use core::panic;
use image::{DynamicImage, ImageFormat, RgbImage};
use orcapod::{
    model::{
        Annotation, Input, InputStoreMapping, OutputStoreMapping, Pod, PodJob, PodOutput,
        PodResult, RetryPolicy, RunMetrics, Status, StorePointer, StreamInfo,
    },
    store::{DataStore, ModelID, ModelInfo, ModelStore},
};
use serde_with::chrono::NaiveDate;
use std::{collections::BTreeMap, io::Cursor, ops::Deref, path::PathBuf};

// --- fixtures ---

pub fn pod_fixture() -> Result<Pod> {
    Ok(Pod::new(
        Some(Annotation {
            name: "style-transfer".to_owned(),
            description: "This is an example pod.".to_owned(),
            version: "0.67.0".to_owned(),
        }),
        "https://github.com/zenml-io/zenml/tree/0.67.0".to_owned(),
        "zenmldocker/zenml-server:0.67.0".to_owned(),
        "tail -f /dev/null".to_owned(),
        BTreeMap::from([
            (
                "painting".to_owned(),
                StreamInfo {
                    path: PathBuf::from("/input/painting.png"),
                    match_pattern: "/input/painting.png".to_owned(),
                },
            ),
            (
                "image".to_owned(),
                StreamInfo {
                    path: PathBuf::from("/input/image.png"),
                    match_pattern: "/input/image.png".to_owned(),
                },
            ),
        ]),
        PathBuf::from("/output"),
        BTreeMap::from([(
            "styled".to_owned(),
            StreamInfo {
                path: PathBuf::from("styled.png"),
                match_pattern: "styled.png".to_owned(),
            },
        )]),
        0.25,                // 250 millicores as frac cores
        (2_u64) * (1 << 30), // 2GiB in bytes
        None,
    )?)
}

static IMAGE_DIM: u32 = 512;

pub fn pod_job_fixture<T: DataStore>(store: &T) -> Result<PodJob> {
    // Generate random uniform image
    let mut img_buffer = RgbImage::new(IMAGE_DIM, IMAGE_DIM);

    for (_, _, pixel) in img_buffer.enumerate_pixels_mut() {
        *pixel = image::Rgb([255, 255, 255]);
    }

    // Covert it to rawbytes
    let mut bytes = Vec::new();
    let img = DynamicImage::from(img_buffer);
    img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)?;

    // Store it in the store if it doesn't exists yet
    store.save_file("style.png", bytes.clone())?;
    store.save_file("image.png", bytes.clone())?;

    // Create the input volume map
    let mut input_volume_map = BTreeMap::new();

    input_volume_map.insert(
        "style".to_owned(),
        Input::FileOrFolder(InputStoreMapping::new("style.png", None)),
    );
    input_volume_map.insert(
        "image".to_owned(),
        Input::FileOrFolder(InputStoreMapping::new("image.png", None)),
    );

    //
    let output_store_mapping = OutputStoreMapping {
        path: "stylized_image".into(),
        store_name: None,
    };

    Ok(PodJob::new(
        Some(Annotation {
            name: "style-transfer-job".to_owned(),
            description: "This is an example pod job.".to_owned(),
            version: "0.67.0".to_owned(),
        }),
        pod_fixture()?,
        input_volume_map,
        output_store_mapping,
        2.0_f32,
        (4_u64) * (1 << 30),
        RetryPolicy::NoRetry,
    )?)
}

/// # Panics
/// Will panic if the date is invalid
pub fn pod_result_fixture<T: ModelStore>(store: &T) -> Result<PodResult> {
    // Generate random uniform image
    let mut img_buffer = RgbImage::new(IMAGE_DIM, IMAGE_DIM);

    for (_, _, pixel) in img_buffer.enumerate_pixels_mut() {
        *pixel = image::Rgb([255, 255, 255]);
    }

    // Create the stylized image
    let mut bytes = Vec::new();
    let img = DynamicImage::from(img_buffer);
    img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)?;

    let pod_output_image_path: PathBuf = PathBuf::from("stylized_image.png");
    let output_folder_name = "2025_1_1-5-6_7";

    let full_rel_save_path = PathBuf::from(&output_folder_name).join(&pod_output_image_path);

    store.save_file(&full_rel_save_path, bytes)?;

    // Run Metrics
    let run_metrics = RunMetrics {
        queued_time: NaiveDate::from_ymd_opt(2025, 1, 1)
            .unwrap()
            .and_hms_opt(1, 1, 1)
            .unwrap(),
        start_time: NaiveDate::from_ymd_opt(2025, 1, 1)
            .unwrap()
            .and_hms_opt(2, 3, 4)
            .unwrap(),
        completed_time: NaiveDate::from_ymd_opt(2025, 1, 1)
            .unwrap()
            .and_hms_opt(5, 6, 7)
            .unwrap(),
        cpu_usage_history: vec![(
            NaiveDate::from_ymd_opt(2025, 1, 1)
                .unwrap()
                .and_hms_opt(5, 6, 7)
                .unwrap(),
            100,
        )],
        mem_usage_history: vec![(
            NaiveDate::from_ymd_opt(2025, 1, 1)
                .unwrap()
                .and_hms_opt(5, 6, 7)
                .unwrap(),
            100,
        )],
    };

    let mut pod_job = pod_job_fixture(store)?;
    store.save_pod_job(&mut pod_job)?;

    let pod_output = PodOutput {
        output_folder_name: output_folder_name.to_owned(),
        checksum: store.compute_checksum_for_path(&full_rel_save_path)?,
        file_list: vec![pod_output_image_path],
    };

    // Create the pod result
    Ok(PodResult::new(
        "testing_orca_img".to_owned(),
        "processing done".to_owned(),
        run_metrics,
        Some(pod_output),
        pod_job,
        Status::Completed,
    )?)
}

pub fn store_pointer_fixture(store: &impl DataStore) -> Result<StorePointer> {
    Ok(StorePointer::new(
        Annotation {
            name: "store 1".to_owned(),
            version: "0.0.0".to_owned(),
            description: "Exmaple store pointer for test usage".to_owned(),
        },
        store.get_uri(),
    )?)
}

// --- util ---
#[derive(Debug)]
pub struct StoreScaffold<T: ModelStore> {
    pub store: T,
}

impl<T: ModelStore> Deref for StoreScaffold<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.store
    }
}

impl<T: ModelStore> Drop for StoreScaffold<T> {
    fn drop(&mut self) {
        self.store.wipe().unwrap();
    }
}

impl<T: ModelStore> StoreScaffold<T> {
    pub fn save_model(&self, model: &mut Model) -> Result<()> {
        match model {
            Model::Pod(pod) => Ok(self.store.save_pod(pod)?),
            Model::PodJob(pod_job) => Ok(self.store.save_pod_job(pod_job)?),
            Model::PodResult(pod_result) => Ok(self.store.save_pod_result(pod_result)?),
            Model::StorePointer(store_pointer) => {
                Ok(self.store.save_store_pointer(store_pointer)?)
            }
        }
    }

    /// # Panics
    /// Panic if try to load model that doesn't have annotation by annotation
    pub fn load_model(&self, model_id: &ModelID, model_type: &ModelType) -> Result<Model> {
        match model_type {
            ModelType::Pod => Ok(Model::Pod(self.store.load_pod(model_id)?)),
            ModelType::PodJob => Ok(Model::PodJob(self.store.load_pod_job(model_id)?)),
            ModelType::PodResult => match model_id {
                ModelID::Hash(hash) => Ok(Model::PodResult(self.load_pod_result(hash)?)),
                ModelID::Annotation(_, _) => {
                    panic!("Invalid use case of load by annotation for pod result")
                }
            },
            ModelType::StorePointer => {
                let store_name = match model_id {
                    ModelID::Hash(_) => {
                        return Err(anyhow!("Store model cannot be loaded by hash only"))
                    }
                    ModelID::Annotation(name, _) => name,
                };
                Ok(Model::StorePointer(
                    self.store.load_store_pointer(store_name)?,
                ))
            }
        }
    }

    pub fn list_model(&self, model_type: &ModelType) -> Result<Vec<ModelInfo>> {
        match model_type {
            ModelType::Pod => Ok(self.store.list_pod()?),
            ModelType::PodJob => Ok(self.store.list_pod_job()?),
            ModelType::PodResult => Ok(self.store.list_pod_result()?),
            ModelType::StorePointer => Ok(self.store.list_store_pointer()?),
        }
    }

    /// # Panics
    /// panics if the type doesn't support delete by annotation and usage attempts to do that
    pub fn delete_item(&self, model_id: &ModelID, model_type: &ModelType) -> Result<()> {
        match model_type {
            ModelType::Pod => Ok(self.store.delete_pod(model_id)?),
            ModelType::PodJob => Ok(self.store.delete_pod_job(model_id)?),
            ModelType::PodResult => match model_id {
                ModelID::Hash(hash) => Ok(self.store.delete_pod_result(hash)?),
                ModelID::Annotation(_, _) => {
                    panic!("Invalid use case of delete by annotation for pod result")
                }
            },
            ModelType::StorePointer => Ok(self.store.delete_store_pointer(model_id)?),
        }
    }

    pub fn delete_item_annotation(
        &self,
        name: &str,
        version: &str,
        model_type: &ModelType,
    ) -> Result<()> {
        match model_type {
            ModelType::Pod => Ok(self.store.delete_annotation::<Pod>(name, version)?),
            ModelType::PodJob => Ok(self.store.delete_annotation::<PodJob>(name, version)?),
            ModelType::PodResult => Ok(self.store.delete_annotation::<PodResult>(name, version)?),
            ModelType::StorePointer => Ok(self
                .store
                .delete_annotation::<StorePointer>(name, version)?),
        }
    }
}

#[derive(PartialEq, Debug)]
pub enum Model {
    Pod(Pod),
    PodJob(PodJob),
    PodResult(PodResult),
    StorePointer(StorePointer),
}

impl Model {
    ///
    /// # Panics
    /// Will panic if annotation is empty
    pub fn get_annotation(&self) -> &Annotation {
        match self {
            Self::Pod(pod) => pod.annotation.as_ref().expect("Pod has empty annotation"),
            Self::PodJob(pod_job) => pod_job
                .annotation
                .as_ref()
                .expect("Pod job has empty annotation"),
            Self::PodResult(_) => panic!("Pod Result does not have annotation"),
            Self::StorePointer(store_pointer) => &store_pointer.annotation,
        }
    }

    pub fn get_hash(&self) -> &str {
        match self {
            Self::Pod(pod) => &pod.hash,
            Self::PodJob(pod_job) => &pod_job.hash,
            Self::PodResult(pod_result) => &pod_result.hash,
            Self::StorePointer(store_pointer) => &store_pointer.hash,
        }
    }

    /// # Panics
    /// Only panic if ``store_pointer`` annotation is None which shouldn't be possiable
    pub fn set_annotation(&mut self, annotation: Option<Annotation>) {
        match self {
            Self::Pod(pod) => pod.annotation = annotation,
            Self::PodJob(pod_job) => pod_job.annotation = annotation,
            Self::PodResult(_) => panic!("Pod Result does not have annotation"),
            // Store pointer cannot have an empty annotation
            Self::StorePointer(store_pointer) => store_pointer.annotation = annotation.unwrap(),
        }
    }

    pub fn set_sub_models_annotation_to_none(&mut self) -> Result<()> {
        match self {
            Self::Pod(_) => Ok(()),
            Self::PodJob(pod_job) => {
                pod_job.pod.annotation = None;
                Ok(())
            }
            Self::PodResult(pod_result) => {
                pod_result.pod_job.annotation = None;
                pod_result.pod_job.pod.annotation = None;
                Ok(())
            }
            Self::StorePointer(_) => Err(anyhow!("Store pointer cannot have None annotation")),
        }
    }
}

impl ModelType {
    pub fn get_model<T: ModelStore + DataStore>(&self, store: &StoreScaffold<T>) -> Result<Model> {
        match self {
            Self::Pod => Ok(Model::Pod(pod_fixture()?)),
            Self::PodJob => Ok(Model::PodJob(pod_job_fixture(&store.store)?)),
            Self::PodResult => Ok(Model::PodResult(pod_result_fixture(&store.store)?)),
            Self::StorePointer => Ok(Model::StorePointer(store_pointer_fixture(&store.store)?)),
        }
    }
}

pub enum ModelType {
    Pod,
    PodJob,
    PodResult,
    StorePointer,
}
