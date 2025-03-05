use crate::{
    error::{Kind, OrcaError, Result},
    model::{Input, Pod, PodJob, PodResult, StoreMap},
    orchestrator::{ImageKind, Orchestrator, PodRun, RunInfo, Status},
};
use bollard::{
    container::{
        Config, CreateContainerOptions, ListContainersOptions, LogOutput, LogsOptions,
        RemoveContainerOptions, StartContainerOptions, WaitContainerOptions,
    },
    image::{CreateImageOptions, ImportImageOptions},
    models::{ContainerStateStatusEnum, HostConfig},
    Docker,
};
use chrono::DateTime;
use futures_util::{
    future::join_all,
    stream::{StreamExt as _, TryStreamExt as _},
};
use names::{Generator, Name};
use regex::Regex;
use std::{collections::HashMap, fs, sync::LazyLock};
use tokio::{fs::File, runtime::Runtime};
use tokio_util::{
    bytes::{Bytes, BytesMut},
    codec::{BytesCodec, FramedRead},
};

/// Support for an orchestration engine using a local docker installation.
#[derive(Debug)]
pub struct LocalDockerOrchestrator {
    api: Docker,
    async_driver: Runtime,
}

impl Orchestrator for LocalDockerOrchestrator {
    fn start_with_altimage_blocking(
        &self,
        pod_job: &PodJob,
        image: &ImageKind,
        store_map: &StoreMap,
    ) -> Result<PodRun> {
        self.async_driver
            .block_on(self.start_with_altimage(pod_job, image, store_map))
    }
    fn start_blocking(&self, pod_job: &PodJob, store_map: &StoreMap) -> Result<PodRun> {
        self.async_driver.block_on(self.start(pod_job, store_map))
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
        pod_job: &PodJob,
        image: &ImageKind,
        store_map: &StoreMap,
    ) -> Result<PodRun> {
        let (assigned_name, container_options, container_config) = match image {
            ImageKind::Published(remote_image) => {
                Self::prepare_container_start_inputs(pod_job, remote_image.clone(), store_map)?
            }
            ImageKind::Tarball(path) => {
                let byte_stream = FramedRead::new(File::open(&path).await?, BytesCodec::new())
                    .map_err(|err| -> Result<BytesMut> {
                        let resolved_error = Err::<BytesMut, OrcaError>(err.into())?;
                        Ok(resolved_error)
                    })
                    .map(|result| result.ok().map_or(Bytes::new(), BytesMut::freeze));
                let mut stream =
                    self.api
                        .import_image_stream(ImportImageOptions::default(), byte_stream, None);
                let mut local_image = String::new();
                while let Some(response) = stream.next().await {
                    local_image = RE_IMAGE_TAG
                        .captures_iter(&response?.stream.ok_or(OrcaError::from(
                            Kind::EmptyResponseWhenLoadingContainerAltImage { path: path.clone() },
                        ))?)
                        .find_map(|x| x.name("image").map(|name| name.as_str().to_owned()))
                        .ok_or(OrcaError::from(Kind::NoTagFoundInContainerAltImage {
                            path: path.clone(),
                        }))?;
                }
                Self::prepare_container_start_inputs(pod_job, local_image.clone(), store_map)?
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
    async fn start(&self, pod_job: &PodJob, store_map: &StoreMap) -> Result<PodRun> {
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
            store_map,
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
            let mut pod: Pod = serde_json::from_str(&run_info.labels["org.orcapod.pod"])?;
            pod.annotation = serde_json::from_str(&run_info.labels["org.orcapod.pod.annotation"])?;
            pod.hash
                .clone_from(&run_info.labels["org.orcapod.pod.hash"]);
            let mut pod_job: PodJob =
                serde_json::from_str(&run_info.labels["org.orcapod.pod_job"])?;
            pod_job.annotation =
                serde_json::from_str(&run_info.labels["org.orcapod.pod_job.annotation"])?;
            pod_job
                .hash
                .clone_from(&run_info.labels["org.orcapod.pod_job.hash"]);
            pod_job.pod = pod;
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

        let mut containers = self
            .list_containers(HashMap::from([("label".to_owned(), labels)]))
            .await?;

        let (_, run_info) = containers
            .next()
            .ok_or(OrcaError::from(Kind::NoMatchingPodRun {
                pod_job_hash: pod_run.pod_job.hash.clone(),
            }))?;

        if containers.next().is_some() {
            return Err(OrcaError::from(Kind::MultipleMatchingPodRunsFound {
                annotation: pod_run.pod_job.annotation.clone(),
                hash: pod_run.pod_job.hash.clone(),
            }));
        }
        Ok(run_info)
    }
    #[expect(
        clippy::let_underscore_must_use,
        reason = "We don't care about the result output here"
    )]
    #[expect(
        clippy::let_underscore_untyped,
        reason = "We don't care about the result output here"
    )]
    async fn get_result(&self, pod_run: &PodRun) -> Result<PodResult> {
        // Wait for the container to complete or fail (ignoring result of wait)
        let _ = self
            .api
            .wait_container(
                &pod_run.assigned_name,
                Some(WaitContainerOptions::<String>::default()),
            )
            .try_collect::<Vec<_>>()
            .await;

        let result_info = self.get_info(pod_run).await?;

        // Get logs (For now it is only doing stdout and doesn't deal with stderr)
        // NOTE: this probably can be improved. Just not sure what is the correct syntax to avoid the two collects
        let logs = String::from_utf8(
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
                .flat_map(|log_output| match log_output {
                    LogOutput::StdOut { message } | LogOutput::StdErr { message } => {
                        message.to_vec()
                    }
                    LogOutput::StdIn { .. } => todo!(),
                    LogOutput::Console { .. } => todo!(),
                })
                .collect::<Vec<u8>>(),
        )?;

        PodResult::new(
            None,
            pod_run.pod_job.clone(),
            pod_run.assigned_name.clone(),
            result_info.status,
            result_info.created,
            result_info.terminated.ok_or(OrcaError::from(
                Kind::InvalidPodResultTerminatedDatetime {
                    pod_job_hash: pod_run.pod_job.hash.clone(),
                },
            ))?,
            logs,
        )
    }
}

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

impl LocalDockerOrchestrator {
    /// How to create a local docker orchestrator with an absolute path on docker host where binds
    /// will be mounted from.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue creating a local docker orchestrator.
    pub fn new() -> Result<Self> {
        Ok(Self {
            api: Docker::connect_with_local_defaults()?,
            async_driver: Runtime::new()?,
        })
    }
    #[expect(
        clippy::cast_possible_wrap,
        clippy::cast_possible_truncation,
        clippy::too_many_lines,
        reason = r#"
        - No issue in memory casting if between 0 - 2^63(i64:MAX, 8EB)
        - No issue in cores casting if in increments of 1e-9(nanocore)
        - This function needs to do a lot
        "#
    )]
    fn prepare_container_start_inputs(
        pod_job: &PodJob,
        image: String,
        store_map: &StoreMap,
    ) -> Result<(
        String,
        Option<CreateContainerOptions<String>>,
        Config<String>,
    )> {
        // Iterate through the inputs and convert them to mounting points for the container
        let mut unary_input_binds = Vec::new();

        for (stream_name, stream_info) in &pod_job.pod.input_stream_map {
            // Find the correct mapping in pod_job.input_stream_mapping and create the mapping string
            match pod_job
                .input_stream_mapping
                .get(stream_name)
                .ok_or_else(|| {
                    OrcaError::from(Kind::MissingStreamInPodJob {
                        stream_name: stream_name.to_owned(),
                    })
                })? {
                Input::Unary(blob) => {
                    // Check if exists first
                    if !blob.resolve_absolute_path(store_map)?.exists() {
                        return Err(OrcaError::from(Kind::InputFileOrFolderNotFound {
                            path: blob.rel_path.clone(),
                        }));
                    }
                    unary_input_binds.push(format!(
                        "{}:{}:ro",
                        blob.resolve_absolute_path(store_map)?.to_string_lossy(),
                        stream_info.path.to_string_lossy(),
                    ));
                }
                Input::Collection(blobs) => {
                    for blob in blobs {
                        // Check if exists first
                        if !blob.resolve_absolute_path(store_map)?.exists() {
                            return Err(OrcaError::from(Kind::InputFileOrFolderNotFound {
                                path: blob.rel_path.clone(),
                            }));
                        }

                        // Mount to the same folder as individual files
                        unary_input_binds.push(format!(
                            "{}:{}:ro",
                            blob.resolve_absolute_path(store_map)?.to_string_lossy(),
                            stream_info
                                .path
                                .join(blob.rel_path.file_name().ok_or_else(|| OrcaError::from(
                                    Kind::FailedToExtractFileName {
                                        path: blob.rel_path.clone()
                                    }
                                ))?)
                                .to_string_lossy(),
                        ));
                    }
                }
            }
        }

        // Ensure output directory exists to prevent permissions issues if daemon's owner is root
        fs::create_dir_all(&pod_job.output_stream_path)?;

        let output_bind = [format!(
            "{}:{}",
            pod_job.output_stream_path.to_string_lossy(),
            pod_job.pod.output_dir.to_string_lossy(),
        )];

        // Prepare configuration
        let container_name = Generator::with_naming(Name::Plain)
            .next()
            .ok_or(OrcaError::from(Kind::GeneratedNamesOverflow))?;
        let labels = HashMap::from([
            ("org.orcapod".to_owned(), "true".to_owned()),
            (
                "org.orcapod.pod.annotation".to_owned(),
                serde_json::to_string(&pod_job.pod.annotation)?,
            ),
            ("org.orcapod.pod.hash".to_owned(), pod_job.pod.hash.clone()),
            (
                "org.orcapod.pod".to_owned(),
                serde_json::to_string(&pod_job.pod)?,
            ),
            (
                "org.orcapod.pod_job.annotation".to_owned(),
                serde_json::to_string(&pod_job.annotation)?,
            ),
            ("org.orcapod.pod_job.hash".to_owned(), pod_job.hash.clone()),
            (
                "org.orcapod.pod_job".to_owned(),
                serde_json::to_string(&pod_job)?,
            ),
        ]);

        let command = pod_job
            .pod
            .command
            .split_whitespace()
            .map(String::from)
            .collect::<Vec<_>>();

        Ok((
            container_name.clone(),
            Some(CreateContainerOptions {
                name: container_name,
                platform: None,
            }),
            Config {
                image: Some(image),
                entrypoint: Some(command[..1].to_vec()),
                cmd: Some(command[1..].to_vec()),
                env: pod_job.env_vars.as_ref().map(|provided_env_vars| {
                    provided_env_vars
                        .iter()
                        .map(|(name, value)| format!("{name}={value}"))
                        .collect()
                }),
                host_config: Some(HostConfig {
                    nano_cpus: Some((pod_job.cpu_limit * 10_f32.powi(9)) as i64), // ncpu, ucores=3, mcores=6, cores=9
                    memory: Some(pod_job.memory_limit as i64),
                    binds: Some([&*unary_input_binds, &output_bind].concat()),
                    ..Default::default()
                }),
                labels: Some(labels),
                ..Default::default()
            },
        ))
    }
    #[expect(
        clippy::cast_sign_loss,
        clippy::string_slice,
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = r#"
        - Timestamp and memory should always have a value > 0
        - Container will always have a name with more than 1 character
        - No issue in core casting if between 0 - 3.40e38(f32:MAX)
        - No issue in exit code casting if between -3.27e4(i16:MIN) - 3.27e4(i16:MAX)
        "#
    )]
    async fn list_containers(
        &self,
        filters: HashMap<String, Vec<String>>, // https://docs.rs/bollard/latest/bollard/container/struct.ListContainersOptions.html#structfield.filters
    ) -> Result<impl Iterator<Item = (String, RunInfo)>> {
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
                    let container_name = &container_summary
                        .names
                        .as_ref()
                        .ok_or(OrcaError::from(Kind::NoContainerNames))?[0][1..];
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
            let (container_name, container_summary, container_spec) = result.ok()?;
            let terminated_timestamp =
                DateTime::parse_from_rfc3339(container_spec.state.as_ref()?.finished_at.as_ref()?)
                    .ok()?
                    .timestamp() as u64;
            Some((
                container_name,
                RunInfo {
                    image: container_spec.config.as_ref()?.image.as_ref()?.clone(),
                    created: container_summary.created? as u64,
                    terminated: (terminated_timestamp > 0).then_some(terminated_timestamp),
                    env_vars: container_spec
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
                    command: format!(
                        "{} {}",
                        container_spec
                            .config
                            .as_ref()?
                            .entrypoint
                            .as_ref()?
                            .join(" "),
                        container_spec.config.as_ref()?.cmd.as_ref()?.join(" ")
                    ),
                    status: match (
                        container_spec.state.as_ref()?.status.as_ref()?,
                        container_spec.state.as_ref()?.exit_code? as i16,
                    ) {
                        (ContainerStateStatusEnum::RUNNING, _) => Status::Running,
                        (ContainerStateStatusEnum::EXITED, 0) => Status::Completed,
                        (ContainerStateStatusEnum::EXITED, code) => Status::Failed(code),
                        _ => todo!(),
                    },
                    mounts: container_spec
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
                    labels: container_spec.config.as_ref()?.labels.as_ref()?.clone(),
                    cpu_limit: container_spec.host_config.as_ref()?.nano_cpus? as f32
                        / 10_f32.powi(9), // ncpu, ucores=3, mcores=6, cores=9
                    memory_limit: container_spec.host_config.as_ref()?.memory? as u64,
                },
            ))
        }))
    }
}
