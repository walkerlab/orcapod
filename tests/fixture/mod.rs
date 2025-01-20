#![expect(clippy::expect_used, reason = "Expect OK in tests.")]
#![expect(clippy::unwrap_used, reason = "Expect test to unwrap without failed")]
#![expect(
    clippy::missing_errors_doc,
    reason = "Integration tests won't be included in documentation."
)]

use anyhow::Result;
use image::{DynamicImage, ImageFormat, RgbImage};
use orcapod::{
    model::{
        Annotation, Input, InputStoreMapping, OutputStoreMapping, Pod, PodJob, RetryPolicy,
        StreamInfo,
    },
    store::{DataStore, ModelID, ModelInfo, ModelStore},
};
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

    // Store it in the store
    store.save_file("style.png", bytes.clone())?;
    store.save_file("image.png", bytes)?;

    // Create the input volume map
    let mut input_volume_map = BTreeMap::new();

    input_volume_map.insert(
        "style".to_owned(),
        Input::File(InputStoreMapping::new("style.png", None)),
    );
    input_volume_map.insert(
        "image".to_owned(),
        Input::File(InputStoreMapping::new("image.png", None)),
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
    pub fn save_model(&self, model: &Model) -> Result<()> {
        match model {
            Model::Pod(pod) => Ok(self.store.save_pod(pod)?),
            Model::PodJob(pod_job) => Ok(self.store.save_pod_job(pod_job)?),
        }
    }

    pub fn load_model(&self, model_id: &ModelID, model_type: &ModelType) -> Result<Model> {
        match model_type {
            ModelType::Pod => Ok(Model::Pod(self.store.load_pod(model_id)?)),
            ModelType::PodJob => Ok(Model::PodJob(self.store.load_pod_job(model_id)?)),
        }
    }

    pub fn list_model(&self, model_type: &ModelType) -> Result<Vec<ModelInfo>> {
        match model_type {
            ModelType::Pod => Ok(self.store.list_pod()?),
            ModelType::PodJob => Ok(self.store.list_pod_job()?),
        }
    }

    pub fn delete_item(&self, model_id: &ModelID, model_type: &ModelType) -> Result<()> {
        match model_type {
            ModelType::Pod => Ok(self.store.delete_pod(model_id)?),
            ModelType::PodJob => Ok(self.store.delete_pod_job(model_id)?),
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
        }
    }
}

#[derive(PartialEq, Debug)]
pub enum Model {
    Pod(Pod),
    PodJob(PodJob),
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
        }
    }

    pub fn get_hash(&self) -> &str {
        match self {
            Self::Pod(pod) => &pod.hash,
            Self::PodJob(pod_job) => &pod_job.hash,
        }
    }

    /// # Panics
    /// Only panic if ``store_pointer`` annotation is None which shouldn't be possiable
    pub fn set_annotation(&mut self, annotation: Option<Annotation>) {
        match self {
            Self::Pod(pod) => pod.annotation = annotation,
            Self::PodJob(pod_job) => pod_job.annotation = annotation,
            // Store pointer cannot have an empty annotation
        }
    }

    pub fn set_sub_models_annotation_to_none(&mut self) -> Result<()> {
        match self {
            Self::Pod(_) => Ok(()),
            Self::PodJob(pod_job) => {
                pod_job.pod.annotation = None;
                Ok(())
            }
        }
    }
}

impl ModelType {
    pub fn get_model<T: ModelStore + DataStore>(&self, store: &StoreScaffold<T>) -> Result<Model> {
        match self {
            Self::Pod => Ok(Model::Pod(pod_fixture()?)),
            Self::PodJob => Ok(Model::PodJob(pod_job_fixture(&store.store)?)),
        }
    }
}

pub enum ModelType {
    Pod,
    PodJob,
}
