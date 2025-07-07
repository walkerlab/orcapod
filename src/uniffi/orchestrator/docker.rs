use crate::{
    core::{
        orchestrator::{ASYNC_RUNTIME, docker::RE_IMAGE_TAG},
        util::get,
    },
    uniffi::{
        error::{OrcaError, Result, selector},
        model::{PodJob, PodResult},
        orchestrator::{ImageKind, Orchestrator, PodRun, RunInfo},
    },
};
use async_trait;
use bollard::{
    Docker,
    container::{RemoveContainerOptions, StartContainerOptions, WaitContainerOptions},
    image::{CreateImageOptions, ImportImageOptions},
};
use derive_more::Display;
use futures_util::stream::{StreamExt as _, TryStreamExt as _};
use snafu::{OptionExt as _, futures::TryFutureExt as _};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::fs::File;
use tokio_util::{
    bytes::{Bytes, BytesMut},
    codec::{BytesCodec, FramedRead},
};
use uniffi;

/// Support for an orchestration engine using a local docker installation.
#[derive(uniffi::Object, Debug, Display)]
#[display("{self:#?}")]
#[uniffi::export(Display)]
pub struct LocalDockerOrchestrator {
    /// API to interact with Docker daemon.
    pub api: Docker,
}

#[uniffi::export(async_runtime = "tokio")]
#[async_trait::async_trait]
impl Orchestrator for LocalDockerOrchestrator {
    fn start_with_altimage_blocking(
        &self,
        namespace_lookup: &HashMap<String, PathBuf>,
        pod_job: &PodJob,
        image: &ImageKind,
    ) -> Result<PodRun> {
        ASYNC_RUNTIME.block_on(self.start_with_altimage(namespace_lookup, pod_job, image))
    }
    fn start_blocking(
        &self,
        namespace_lookup: &HashMap<String, PathBuf>,
        pod_job: &PodJob,
    ) -> Result<PodRun> {
        ASYNC_RUNTIME.block_on(self.start(namespace_lookup, pod_job))
    }
    fn list_blocking(&self) -> Result<Vec<PodRun>> {
        ASYNC_RUNTIME.block_on(self.list())
    }
    fn delete_blocking(&self, pod_run: &PodRun) -> Result<()> {
        ASYNC_RUNTIME.block_on(self.delete(pod_run))
    }
    fn get_info_blocking(&self, pod_run: &PodRun) -> Result<RunInfo> {
        ASYNC_RUNTIME.block_on(self.get_info(pod_run))
    }
    fn get_result_blocking(&self, pod_run: &PodRun) -> Result<PodResult> {
        ASYNC_RUNTIME.block_on(self.get_result(pod_run))
    }
    #[expect(
        clippy::try_err,
        reason = r#"
        - `map_err` workaround needed since `import_image_stream` requires resolved bytes
        - Raising an error manually on occurrence to halt so we don't just ignore
        - Should not get as far as `Ok(_)`
        "#
    )]
    async fn start_with_altimage(
        &self,
        namespace_lookup: &HashMap<String, PathBuf>,
        pod_job: &PodJob,
        image: &ImageKind,
    ) -> Result<PodRun> {
        let (assigned_name, container_options, container_config) = match image {
            ImageKind::Published(remote_image) => Self::prepare_container_start_inputs(
                namespace_lookup,
                pod_job,
                remote_image.clone(),
            )?,
            ImageKind::Tarball(image_info) => {
                let location = namespace_lookup[&image_info.namespace].join(&image_info.path);
                let byte_stream = FramedRead::new(
                    File::open(&location)
                        .context(selector::InvalidFilepath { path: &location })
                        .await?,
                    BytesCodec::new(),
                )
                .map_err(|err| -> Result<()> {
                    Err::<BytesMut, OrcaError>(err.into())?; // raise on error since we discard below
                    Ok(())
                })
                .map(|result| result.ok().map_or(Bytes::new(), BytesMut::freeze));
                let mut stream =
                    self.api
                        .import_image_stream(ImportImageOptions::default(), byte_stream, None);
                let mut local_image = String::new();
                while let Some(response) = stream.next().await {
                    local_image = RE_IMAGE_TAG
                        .captures_iter(&response?.stream.context(
                            selector::EmptyResponseWhenLoadingContainerAltImage {
                                path: location.clone(),
                            },
                        )?)
                        .find_map(|x| x.name("image").map(|name| name.as_str().to_owned()))
                        .context(selector::NoTagFoundInContainerAltImage {
                            path: location.clone(),
                        })?;
                }
                Self::prepare_container_start_inputs(
                    namespace_lookup,
                    pod_job,
                    local_image.clone(),
                )?
            }
        };
        self.api
            .create_container(container_options, container_config)
            .await?;
        self.api
            .start_container(&assigned_name, None::<StartContainerOptions<String>>)
            .await?;
        Ok(PodRun::new::<Self>(pod_job, assigned_name))
    }
    async fn start(
        &self,
        namespace_lookup: &HashMap<String, PathBuf>,
        pod_job: &PodJob,
    ) -> Result<PodRun> {
        let image_options = Some(CreateImageOptions {
            from_image: pod_job.pod.image.clone(),
            ..Default::default()
        });
        self.api
            .create_image(image_options, None, None)
            .try_collect::<Vec<_>>()
            .await?;
        self.start_with_altimage(
            namespace_lookup,
            pod_job,
            &ImageKind::Published(pod_job.pod.image.clone()),
        )
        .await
    }
    async fn list(&self) -> Result<Vec<PodRun>> {
        self.list_containers(HashMap::from([(
            "label".to_owned(),
            vec!["org.orcapod=true".to_owned()],
        )]))
        .await?
        .map(|(assigned_name, run_info)| {
            let pod_job: PodJob =
                serde_json::from_str(get(&run_info.labels, &"org.orcapod.pod_job".to_owned())?)?;
            Ok(PodRun::new::<Self>(&pod_job, assigned_name))
        })
        .collect()
    }
    async fn delete(&self, pod_run: &PodRun) -> Result<()> {
        self.api
            .remove_container(
                &pod_run.assigned_name,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await?;
        Ok(())
    }
    async fn get_info(&self, pod_run: &PodRun) -> Result<RunInfo> {
        let labels = vec![
            "org.orcapod=true".to_owned(),
            format!(
                "org.orcapod.pod_job.annotation={}",
                serde_json::to_string(&pod_run.pod_job.annotation)?
            ),
            format!("org.orcapod.pod_job.hash={}", pod_run.pod_job.hash),
        ];
        let (_, run_info) = self
            .list_containers(HashMap::from([("label".to_owned(), labels)]))
            .await?
            .next()
            .context(selector::NoMatchingPodRun {
                pod_job_hash: pod_run.pod_job.hash.clone(),
            })?;
        Ok(run_info)
    }
    async fn get_result(&self, pod_run: &PodRun) -> Result<PodResult> {
        self.api
            .wait_container(&pod_run.assigned_name, None::<WaitContainerOptions<String>>)
            .try_collect::<Vec<_>>()
            .await?;
        let result_info = self.get_info(pod_run).await?;
        PodResult::new(
            None,
            Arc::clone(&pod_run.pod_job),
            pod_run.assigned_name.clone(),
            result_info.status,
            result_info.created,
            result_info
                .terminated
                .context(selector::InvalidPodResultTerminatedDatetime {
                    pod_job_hash: pod_run.pod_job.hash.clone(),
                })?,
        )
    }
}

#[uniffi::export]
impl LocalDockerOrchestrator {
    /// How to create a local docker orchestrator with an absolute path on docker host where binds
    /// will be mounted from.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue creating a local docker orchestrator.
    #[uniffi::constructor]
    pub fn new() -> Result<Self> {
        Ok(Self {
            api: Docker::connect_with_local_defaults()?,
        })
    }
}
