use crate::{
    core::{orchestrator::docker::RE_IMAGE_TAG, util::get},
    uniffi::{
        error::{OrcaError, Result, selector},
        model::{Pod, PodJob, PodResult},
        orchestrator::{ImageKind, Orchestrator, PodRun, RunInfo},
    },
};
use async_trait;
use bollard::{
    Docker,
    container::{LogOutput, LogsOptions, StartContainerOptions, WaitContainerOptions},
    image::{CreateImageOptions, ImportImageOptions},
};
use colored::Colorize as _;
use derive_more::Display;
use futures_util::stream::{StreamExt as _, TryStreamExt as _};
use snafu::{OptionExt as _, ResultExt as _, futures::TryFutureExt as _};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::{fs::File, runtime::Runtime};
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
    async_driver: Runtime,
}

#[uniffi::export]
#[async_trait::async_trait]
impl Orchestrator for LocalDockerOrchestrator {
    fn start_with_altimage_blocking(
        &self,
        namespace_lookup: &HashMap<String, PathBuf>,
        pod_job: &PodJob,
        image: &ImageKind,
    ) -> Result<PodRun> {
        self.async_driver
            .block_on(self.start_with_altimage(namespace_lookup, pod_job, image))
    }
    fn start_blocking(
        &self,
        namespace_lookup: &HashMap<String, PathBuf>,
        pod_job: &PodJob,
    ) -> Result<PodRun> {
        self.async_driver
            .block_on(self.start(namespace_lookup, pod_job))
    }
    fn list_blocking(&self) -> Result<Vec<PodRun>> {
        self.async_driver.block_on(self.list())
    }
    fn delete_blocking(&self, pod_run: &PodRun) -> Result<()> {
        self.async_driver.block_on(self.delete(pod_run))
    }
    fn get_info_blocking(&self, pod_run: &PodRun) -> Result<RunInfo> {
        self.async_driver.block_on(self.get_info(pod_run))
    }
    fn get_result_blocking(&self, pod_run: &PodRun) -> Result<PodResult> {
        self.async_driver.block_on(self.get_result(pod_run))
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
                        .context(selector::InvalidFileOrDirPath { path: &location })
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
            .await
            .context(selector::FailedToStartPod {
                container_name: assigned_name.clone(),
            })?;

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
        .map(|result| {
            let (assigned_name, run_info) = result?;
            let mut pod: Pod =
                serde_json::from_str(get(&run_info.labels, &"org.orcapod.pod".to_owned())?)?;
            pod.annotation = serde_json::from_str(get(
                &run_info.labels,
                &"org.orcapod.pod.annotation".to_owned(),
            )?)?;
            pod.hash
                .clone_from(get(&run_info.labels, &"org.orcapod.pod.hash".to_owned())?);
            let mut pod_job: PodJob =
                serde_json::from_str(get(&run_info.labels, &"org.orcapod.pod_job".to_owned())?)?;
            pod_job.annotation = serde_json::from_str(get(
                &run_info.labels,
                &"org.orcapod.pod_job.annotation".to_owned(),
            )?)?;
            pod_job.hash.clone_from(get(
                &run_info.labels,
                &"org.orcapod.pod_job.hash".to_owned(),
            )?);
            pod_job.pod = pod.into();
            Ok(PodRun::new::<Self>(&pod_job, assigned_name))
        })
        .collect()
    }
    async fn delete(&self, pod_run: &PodRun) -> Result<()> {
        self.delete_container(&pod_run.assigned_name).await
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

        // Add names to the filters
        let container_filters = HashMap::from([
            ("label".to_owned(), labels),
            (
                "name".to_owned(),
                Vec::from([pod_run.assigned_name.clone()]),
            ),
        ]);

        let (_, run_info) = self
            .list_containers(container_filters)
            .await?
            .next()
            .context(selector::NoMatchingPodRun {
                pod_job_hash: pod_run.pod_job.hash.clone(),
            })??;
        Ok(run_info)
    }
    async fn get_result(&self, pod_run: &PodRun) -> Result<PodResult> {
        match self
            .api
            .wait_container(&pod_run.assigned_name, None::<WaitContainerOptions<String>>)
            .try_collect::<Vec<_>>()
            .await
        {
            Ok(_) => {}
            Err(error) => println!(
                "{}{}",
                "Warning: ".bright_yellow(),
                error.to_string().bright_cyan()
            ),
        }
        let result_info = self.get_info(pod_run).await?;

        let mut std_out = Vec::new();
        let mut std_err = Vec::new();

        self.api
            .logs::<String>(
                &pod_run.assigned_name,
                Some(LogsOptions {
                    stdout: true,
                    stderr: true,
                    ..Default::default()
                }),
            )
            .try_collect::<Vec<_>>()
            .await?
            .iter()
            .for_each(|log_output| match log_output {
                LogOutput::StdOut { message } => {
                    std_out.extend(message.to_vec());
                }
                LogOutput::StdErr { message } => {
                    std_err.extend(message.to_vec());
                }
                LogOutput::StdIn { .. } => todo!(),
                LogOutput::Console { .. } => todo!(),
            });

        let mut logs = String::from_utf8_lossy(&std_out).to_string();
        if !std_err.is_empty() {
            logs.push_str("\nSTDERR:\n");
            logs.push_str(&String::from_utf8_lossy(&std_err));
        }

        // Check for errors, if exist, attach it to logs
        let error = self
            .api
            .inspect_container(&pod_run.assigned_name, None)
            .await?
            .state
            .context(selector::FailedToExtractRunInfo {
                container_name: &pod_run.assigned_name,
            })?
            .error
            .context(selector::FailedToExtractRunInfo {
                container_name: &pod_run.assigned_name,
            })?;

        if !error.is_empty() {
            logs.push_str(&error);
        }

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
            logs,
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
            async_driver: Runtime::new()?,
        })
    }
}
