use crate::{
    core::{
        orchestrator::{ASYNC_RUNTIME, docker::RE_IMAGE_TAG},
        util::get,
    },
    uniffi::{
        error::{Kind, OrcaError, Result, selector},
        model::pod::{PodJob, PodResult},
        orchestrator::{ImageKind, Orchestrator, PodRun, PodRunInfo, PodStatus},
    },
};
use async_trait;
use bollard::{
    Docker,
    container::{
        LogOutput, LogsOptions, RemoveContainerOptions, StartContainerOptions, WaitContainerOptions,
    },
    errors::Error::DockerContainerWaitError,
    image::{CreateImageOptions, ImportImageOptions},
};
use derive_more::Display;
use futures_util::stream::{StreamExt as _, TryStreamExt as _};
use snafu::{OptionExt as _, futures::TryFutureExt as _};
use std::{backtrace::Backtrace, collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::{fs::File, time::sleep as async_sleep};
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
        pod_job: &PodJob,
        image: &ImageKind,
        namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<PodRun> {
        ASYNC_RUNTIME.block_on(self.start_with_altimage(pod_job, image, namespace_lookup))
    }
    fn start_blocking(
        &self,
        pod_job: &PodJob,
        namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<PodRun> {
        ASYNC_RUNTIME.block_on(self.start(pod_job, namespace_lookup))
    }
    fn list_blocking(&self) -> Result<Vec<PodRun>> {
        ASYNC_RUNTIME.block_on(self.list())
    }
    fn delete_blocking(&self, pod_run: &PodRun) -> Result<()> {
        ASYNC_RUNTIME.block_on(self.delete(pod_run))
    }
    fn get_info_blocking(&self, pod_run: &PodRun) -> Result<PodRunInfo> {
        ASYNC_RUNTIME.block_on(self.get_info(pod_run))
    }
    fn get_result_blocking(
        &self,
        pod_run: &PodRun,
        namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<PodResult> {
        ASYNC_RUNTIME.block_on(self.get_result(pod_run, namespace_lookup))
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
        pod_job: &PodJob,
        image: &ImageKind,
        namespace_lookup: &HashMap<String, PathBuf>,
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
                        .captures_iter(&response?.stream.context(selector::MissingInfo {
                            details: location.to_string_lossy(),
                        })?)
                        .find_map(|x| x.name("image").map(|name| name.as_str().to_owned()))
                        .context(selector::MissingInfo {
                            details: format!(
                                "container tags in provided container alternate image where path = {}",
                                location.to_string_lossy()
                            ),
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
        match self
            .api
            .start_container(&assigned_name, None::<StartContainerOptions<String>>)
            .await
        {
            Ok(()) => {}
            Err(err) => Err(OrcaError {
                kind: Kind::FailedToStartPod {
                    container_name: assigned_name.clone(),
                    reason: err.to_string(),
                    backtrace: Backtrace::capture().into(),
                },
            })?,
        }

        Ok(PodRun::new::<Self>(pod_job, assigned_name))
    }
    async fn start(
        &self,
        pod_job: &PodJob,
        namespace_lookup: &HashMap<String, PathBuf>,
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
            pod_job,
            &ImageKind::Published(pod_job.pod.image.clone()),
            namespace_lookup,
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
                serde_json::from_str(get(&run_info.labels, "org.orcapod.pod_job")?)?;
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
    async fn get_info(&self, pod_run: &PodRun) -> Result<PodRunInfo> {
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
            .context(selector::MissingInfo {
                details: format!("pod run where pod_job.hash = {}", pod_run.pod_job.hash),
            })?;
        Ok(run_info)
    }
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Favor readability due to complexity in external dependency."
    )]
    async fn get_result(
        &self,
        pod_run: &PodRun,
        namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<PodResult> {
        match self
            .api
            .wait_container(&pod_run.assigned_name, None::<WaitContainerOptions<String>>)
            .try_collect::<Vec<_>>()
            .await
        {
            Ok(_) => (),
            Err(err) => match err {
                DockerContainerWaitError { .. } => (),
                _ => return Err(OrcaError::from(err)),
            },
        }

        let mut result_info: PodRunInfo;
        while {
            result_info = self.get_info(pod_run).await?;
            matches!(&result_info.status, PodStatus::Running)
        } {
            async_sleep(Duration::from_millis(100)).await;
        }

        PodResult::new(
            None,
            Arc::clone(&pod_run.pod_job),
            pod_run.assigned_name.clone(),
            result_info.status,
            result_info.created,
            result_info.terminated.context(selector::MissingInfo {
                details: format!(
                    "terminated where pod_run.assigned_name = {}, pod_run.pod_job.hash = {}",
                    pod_run.assigned_name, pod_run.pod_job.hash
                ),
            })?,
            namespace_lookup,
            self.get_logs(pod_run).await?,
        )
    }

    async fn get_logs(&self, pod_run: &PodRun) -> Result<String> {
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
                LogOutput::StdIn { .. } | LogOutput::Console { .. } => {
                    // Ignore stdin logs, as they are not relevant for our use case
                }
            });

        let mut logs = String::from_utf8_lossy(&std_out).to_string();
        if !std_err.is_empty() {
            logs.push_str("\nSTDERR:\n");
            logs.push_str(&String::from_utf8_lossy(&std_err));
        }

        // Check for errors in the docker state, if exist, attach it to logs
        // This is for when the container exits immediately due to a bad command or similar
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

        Ok(logs)
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
