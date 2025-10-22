use crate::{
    error::{Kind, OrcaError, Result, selector},
    model::{
        packet::PathSet,
        pod::{PodJob, PodResult, PodStatus},
    },
    orchestrator::{ASYNC_RUNTIME, ImageKind, Orchestrator, PodRun, PodRunInfo},
    util::get,
};
use bollard::{
    Docker,
    container::{
        Config, CreateContainerOptions, ListContainersOptions, LogOutput, LogsOptions,
        RemoveContainerOptions, StartContainerOptions, WaitContainerOptions,
    },
    errors::Error::DockerContainerWaitError,
    image::{CreateImageOptions, ImportImageOptions},
    models::{ContainerStateStatusEnum, HostConfig},
    secret::{ContainerInspectResponse, ContainerSummary},
};
use chrono::DateTime;
use derive_more::Display;
use futures_util::{
    future::join_all,
    stream::{StreamExt as _, TryStreamExt as _},
};
use names::{Generator, Name};
use regex::Regex;
use snafu::{OptionExt as _, futures::TryFutureExt as _};
use std::{
    backtrace::Backtrace,
    collections::HashMap,
    fs,
    path::{self, PathBuf},
    sync::{Arc, LazyLock},
    time::Duration,
};
use tokio::{fs::File, time::sleep as async_sleep};
use tokio_util::{
    bytes::{Bytes, BytesMut},
    codec::{BytesCodec, FramedRead},
};
use uniffi;

#[expect(clippy::expect_used, reason = "Valid static regex")]
static RE_IMAGE_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
                \s
                    (?<image>[^\s]+:[^\s]+)
                \s
            ",
    )
    .expect("Invalid image tag regex.")
});

/// Support for an orchestration engine using a local docker installation.
#[derive(uniffi::Object, Debug, Display)]
#[display("{self:#?}")]
#[uniffi::export(Display)]
pub struct LocalDockerOrchestrator {
    /// API to interact with Docker daemon.
    pub api: Docker,
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

impl LocalDockerOrchestrator {
    fn prepare_mount_binds(
        namespace_lookup: &HashMap<String, PathBuf>,
        pod_job: &PodJob,
    ) -> Result<(Vec<String>, [String; 1])> {
        // all host mounted paths need to be absolute
        let host_output_directory = path::absolute(
            namespace_lookup[&pod_job.output_dir.namespace].join(&pod_job.output_dir.path),
        )?;
        // Ensure output directory exists to prevent permissions issues if daemon's owner is root
        fs::create_dir_all(&host_output_directory)?;
        let output_bind = [format!(
            "{}:{}",
            host_output_directory.to_string_lossy(),
            pod_job.pod.output_dir.to_string_lossy(),
        )];
        let input_binds = pod_job.pod.input_spec.iter().try_fold::<_, _, Result<_>>(
            vec![],
            |mut flattened_binds, (stream_name, stream_info)| {
                flattened_binds.extend(match get(&pod_job.input_packet, stream_name)? {
                    PathSet::Unary(blob) => {
                        vec![format!(
                            "{}:{}:{}",
                            path::absolute(
                                get(namespace_lookup, &blob.location.namespace)?
                                    .join(&blob.location.path)
                            )?
                            .to_string_lossy(),
                            stream_info.path.to_string_lossy(),
                            "ro"
                        )]
                    }
                    PathSet::Collection(blobs) => blobs
                        .iter()
                        .map(|blob| {
                            Ok(format!(
                                "{}:{}:{}",
                                path::absolute(
                                    get(namespace_lookup, &blob.location.namespace)?
                                        .join(&blob.location.path)
                                )?
                                .to_string_lossy(),
                                stream_info
                                    .path
                                    .join(blob.location.path.file_name().context(
                                        selector::MissingInfo {
                                            details: format!(
                                                "file or directory name where path = {}",
                                                blob.location.path.to_string_lossy()
                                            ),
                                        }
                                    )?)
                                    .to_string_lossy(),
                                "ro"
                            ))
                        })
                        .collect::<Result<_>>()?,
                });
                Ok(flattened_binds)
            },
        )?;
        Ok((input_binds, output_bind))
    }
    #[expect(
        clippy::cast_possible_wrap,
        clippy::cast_possible_truncation,
        clippy::indexing_slicing,
        reason = r#"
        - No issue in memory casting if between 0 - 2^63(i64:MAX, 8EB)
        - No issue in cores casting if in increments of 1e-9(nanocore)
        - Pod commands will always have at least 1 element
        "#
    )]
    pub(crate) fn prepare_container_start_inputs(
        namespace_lookup: &HashMap<String, PathBuf>,
        pod_job: &PodJob,
        image: String,
    ) -> Result<(
        String,
        Option<CreateContainerOptions<String>>,
        Config<String>,
    )> {
        // Prepare configuration
        let (input_binds, output_bind) = Self::prepare_mount_binds(namespace_lookup, pod_job)?;
        let container_name =
            Generator::with_naming(Name::Plain)
                .next()
                .context(selector::MissingInfo {
                    details: "unable to generate a random name",
                })?;
        let labels = HashMap::from([
            ("org.orcapod".to_owned(), "true".to_owned()),
            (
                "org.orcapod.pod_job".to_owned(),
                serde_json::to_string(&pod_job)?,
            ),
            (
                "org.orcapod.pod_job.annotation".to_owned(),
                serde_json::to_string(&pod_job.annotation)?,
            ),
            ("org.orcapod.pod_job.hash".to_owned(), pod_job.hash.clone()),
        ]);

        Ok((
            container_name.clone(),
            Some(CreateContainerOptions {
                name: container_name,
                platform: None,
            }),
            Config {
                image: Some(image),
                entrypoint: Some(pod_job.pod.command[..1].to_vec()),
                cmd: Some(pod_job.pod.command[1..].to_vec()),
                env: pod_job.env_vars.as_ref().map(|provided_env_vars| {
                    provided_env_vars
                        .iter()
                        .map(|(name, value)| format!("{name}={value}"))
                        .collect()
                }),
                host_config: Some(HostConfig {
                    nano_cpus: Some((pod_job.cpu_limit * 10_f32.powi(9)) as i64), // ncpu, ucores=3, mcores=6, cores=9
                    memory: Some(pod_job.memory_limit as i64),
                    binds: Some([&*input_binds, &output_bind].concat()),
                    ..Default::default()
                }),
                labels: Some(labels),
                ..Default::default()
            },
        ))
    }
    #[expect(
        clippy::string_slice,
        clippy::indexing_slicing,
        reason = r#"
        - Timestamp and memory should always have a value > 0
        - Container will always have a name with more than 1 character
        - No issue in core casting if between 0 - 3.40e38(f32:MAX)
        - No issue in exit code casting if between -3.27e4(i16:MIN) - 3.27e4(i16:MAX)
        - Containers will always have at least 1 name with at least 2 characters
        "#
    )]
    pub(crate) async fn list_containers(
        &self,
        filters: HashMap<String, Vec<String>>, // https://docs.rs/bollard/latest/bollard/container/struct.ListContainersOptions.html#structfield.filters
    ) -> Result<impl Iterator<Item = (String, PodRunInfo)>> {
        Ok(join_all(
            self.api
                .list_containers(Some(ListContainersOptions {
                    all: true,
                    filters,
                    ..Default::default()
                }))
                .await?
                .iter()
                .map(|container_summary| async {
                    let container_name =
                        &container_summary
                            .names
                            .as_ref()
                            .context(selector::MissingInfo {
                                details: "container name(s)".to_owned(),
                            })?[0][1..];
                    Ok((
                        container_name.to_owned(),
                        container_summary.clone(),
                        self.api.inspect_container(container_name, None).await?,
                    ))
                }),
        )
        .await
        .into_iter()
        .filter_map(|result: Result<_>| {
            let (container_name, container_summary, container_inspect_response) = result.ok()?;

            Self::extract_run_info(&container_summary, &container_inspect_response)
                .map(|run_info| (container_name.clone(), run_info))
        }))
    }

    #[expect(
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = r#"
        - Timestamp and memory should always have a value > 0
        - Container will always have a name with more than 1 character
        - No issue in core casting if between 0 - 3.40e38(f32:MAX)
        - No issue in exit code casting if between -3.27e4(i16:MIN) - 3.27e4(i16:MAX)
        - Containers will always have at least 1 name with at least 2 characters
        - This functions requires a lot of boilerplate code to extract the run info
        "#
    )]
    fn extract_run_info(
        container_summary: &ContainerSummary,
        container_inspect_response: &ContainerInspectResponse,
    ) -> Option<PodRunInfo> {
        let terminated_timestamp = DateTime::parse_from_rfc3339(
            container_inspect_response
                .state
                .as_ref()?
                .finished_at
                .as_ref()?,
        )
        .ok()?
        .timestamp() as u64;
        Some(PodRunInfo {
            image: container_inspect_response
                .config
                .as_ref()?
                .image
                .as_ref()?
                .clone(),
            created: container_summary.created? as u64,
            terminated: (terminated_timestamp > 0).then_some(terminated_timestamp),
            env_vars: container_inspect_response
                .config
                .as_ref()?
                .env
                .as_ref()?
                .iter()
                .filter_map(|x| {
                    x.split_once('=')
                        .map(|(key, value)| (key.to_owned(), value.to_owned()))
                })
                .collect(),
            command: [
                container_inspect_response
                    .config
                    .as_ref()?
                    .entrypoint
                    .as_ref()?
                    .clone(),
                container_inspect_response
                    .config
                    .as_ref()?
                    .cmd
                    .as_ref()?
                    .clone(),
            ]
            .concat(),
            status: match (
                container_inspect_response.state.as_ref()?.status?,
                container_inspect_response.state.as_ref()?.exit_code? as i16,
            ) {
                (ContainerStateStatusEnum::RUNNING | ContainerStateStatusEnum::RESTARTING, _) => {
                    PodStatus::Running
                }
                (ContainerStateStatusEnum::EXITED, 0) => PodStatus::Completed,
                (ContainerStateStatusEnum::EXITED | ContainerStateStatusEnum::DEAD, code) => {
                    PodStatus::Failed(code)
                }
                (ContainerStateStatusEnum::CREATED, code) => {
                    if container_inspect_response.state.as_ref()?.error.is_some() {
                        PodStatus::Failed(code)
                    } else {
                        PodStatus::Running
                    }
                }
                _ => PodStatus::Undefined,
            },
            mounts: container_inspect_response
                .mounts
                .as_ref()?
                .iter()
                .map(|mount_point| {
                    Some(format!(
                        "{}:{}{}",
                        mount_point.source.as_ref()?,
                        mount_point.destination.as_ref()?,
                        mount_point
                            .mode
                            .as_ref()
                            .map_or_else(String::new, |mode| format!(":{mode}"))
                    ))
                })
                .collect::<Option<Vec<_>>>()?,
            labels: container_inspect_response
                .config
                .as_ref()?
                .labels
                .as_ref()?
                .clone(),
            cpu_limit: container_inspect_response.host_config.as_ref()?.nano_cpus? as f32
                / 10_f32.powi(9), // ncpu, ucores=3, mcores=6, cores=9
            memory_limit: container_inspect_response.host_config.as_ref()?.memory? as u64,
        })
    }
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
    fn get_logs_blocking(&self, pod_run: &PodRun) -> Result<String> {
        ASYNC_RUNTIME.block_on(self.get_logs(pod_run))
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
                        .context(selector::InvalidPath { path: &location })
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
