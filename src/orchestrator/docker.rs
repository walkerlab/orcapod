use crate::{
    error::{Kind, OrcaError, Result},
    model::{Input, Pod, PodJob, PodResult},
    orchestrator::{self, ImageKind, PodRun, PodRunAPI, RunInfo, RunState, Types},
};
use bollard::{
    container::{
        Config, CreateContainerOptions, ListContainersOptions, RemoveContainerOptions,
        StartContainerOptions, WaitContainerOptions,
    },
    image::{CreateImageOptions, ImportImageOptions},
    models::{ContainerStateStatusEnum, HostConfig},
    Docker,
};
use chrono::DateTime;
use futures_util::{
    future::join_all,
    stream::{StreamExt, TryStreamExt},
};
use names::{Generator, Name};
use once_cell::sync::Lazy;
use regex::Regex;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use tokio::{fs::File, runtime::Runtime};
use tokio_util::{
    bytes::{Bytes, BytesMut},
    codec::{BytesCodec, FramedRead},
};

/// Support for an orchestration engine using a local docker installation.
#[derive(Debug)]
pub struct LocalDockerOrchestrator {
    data_directory: PathBuf,
    api: Docker,
    async_driver: Runtime,
}

impl Types for PodRun<'_, LocalDockerOrchestrator> {
    type Orchestrator = LocalDockerOrchestrator;
}

impl Types for LocalDockerOrchestrator {
    type Orchestrator = Self;
}

impl PodRunAPI for PodRun<'_, LocalDockerOrchestrator> {
    fn get_info(&self) -> Result<RunInfo> {
        self.orchestrator
            .async_driver
            .block_on(self.get_info_async())
    }
    fn get_result(&self) -> Result<PodResult> {
        self.orchestrator
            .async_driver
            .block_on(self.get_result_async())
    }
    async fn get_result_async(&self) -> Result<PodResult> {
        let run_info = self.get_info_async().await?;
        self.orchestrator
            .api
            .wait_container(&run_info.name, None::<WaitContainerOptions<String>>)
            .try_collect::<Vec<_>>()
            .await?;
        let result_info = self.get_info_async().await?;
        PodResult::new(
            None,
            self.pod_job.clone(),
            result_info.name,
            result_info.state,
            result_info.created,
            result_info.terminated.ok_or(OrcaError::from(
                Kind::InvalidPodResultTerminatedDatetime {
                    pod_job_hash: self.pod_job.hash.clone(),
                },
            ))?,
        )
    }
}

impl orchestrator::API for LocalDockerOrchestrator {
    fn list(&self) -> Result<Vec<impl PodRunAPI>> {
        self.async_driver
            .block_on(self.list_containers(HashMap::from([(
                "label".to_owned(),
                vec!["org.orcapod=true".to_owned()],
            )])))?
            .map(|run_info| {
                let mut pod: Pod = serde_json::from_str(&run_info.labels["org.orcapod.pod"])?;
                pod.annotation =
                    serde_json::from_str(&run_info.labels["org.orcapod.pod.annotation"])?;
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
                PodRun::new(pod_job, self)
            })
            .collect()
    }
    fn start_with_altimage(&self, pod_job: &PodJob, image: &ImageKind) -> Result<impl PodRunAPI> {
        self.async_driver
            .block_on(self.start_with_altimage_async(pod_job, image))
    }
    fn start(&self, pod_job: &PodJob) -> Result<impl PodRunAPI> {
        let image_options = Some(CreateImageOptions {
            from_image: pod_job.pod.image.clone(),
            ..Default::default()
        });
        self.async_driver.block_on(async {
            self.api
                .create_image(image_options, None, None)
                .try_collect::<Vec<_>>()
                .await?;
            self.start_with_altimage_async(
                pod_job,
                &ImageKind::Published(pod_job.pod.image.clone()),
            )
            .await
        })?;
        PodRun::new(pod_job.clone(), self)
    }
    fn delete(&self, pod_run: &impl PodRunAPI) -> Result<()> {
        self.async_driver.block_on(self.api.remove_container(
            &pod_run.get_info()?.name,
            Some(RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        ))?;
        Ok(())
    }
}

impl PodRun<'_, LocalDockerOrchestrator> {
    async fn get_info_async(&self) -> Result<RunInfo> {
        let labels = vec![
            "org.orcapod=true".to_owned(),
            format!(
                "org.orcapod.pod_job.annotation={}",
                serde_json::to_string(&self.pod_job.annotation)?
            ),
            format!("org.orcapod.pod_job.hash={}", self.pod_job.hash),
        ];
        self.orchestrator
            .list_containers(HashMap::from([("label".to_owned(), labels)]))
            .await?
            .next()
            .ok_or(OrcaError::from(Kind::NoMatchingPodRun {
                pod_job_hash: self.pod_job.hash.clone(),
            }))
    }
}

#[expect(clippy::unwrap_used, reason = "Valid static regex")]
static RE_IMAGE_TAGS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?x)
                \s
                    (?<image>[^\s]+:[^\s]+)
                \s
            ",
    )
    .unwrap()
});

impl LocalDockerOrchestrator {
    /// How to create a local docker orchestrator with an absolute path on docker host where binds
    /// will be mounted from.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue creating a local docker orchestrator.
    pub fn new(data_directory: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            data_directory: fs::canonicalize(data_directory)?,
            api: Docker::connect_with_local_defaults()?,
            async_driver: Runtime::new()?,
        })
    }
    #[expect(
        clippy::try_err,
        reason = r#"
        - `map_err` workaround needed since `import_image_stream` requires resolved bytes
        - Raising an errors manually on occurrence to halt so we don't just ignore
        - Should not get as far as `Ok(_)`
    "#
    )]
    async fn start_with_altimage_async(
        &self,
        pod_job: &PodJob,
        image: &ImageKind,
    ) -> Result<impl PodRunAPI + use<'_>> {
        let (container_name, container_options, container_config) = match image {
            ImageKind::Published(remote_image) => {
                self.prepare_container_start_inputs(pod_job, remote_image.clone())?
            }
            ImageKind::Tarball(location) => {
                let byte_stream = FramedRead::new(File::open(location).await?, BytesCodec::new())
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
                    local_image = RE_IMAGE_TAGS
                        .captures_iter(&response?.stream.ok_or(OrcaError::from(
                            Kind::EmptyResponseWhenLoadingContainerAltImage {
                                path: location.clone(),
                            },
                        ))?)
                        .find_map(|x| x.name("image").map(|name| name.as_str().to_owned()))
                        .ok_or(OrcaError::from(Kind::NoTagFoundInContainerAltImage {
                            path: location.clone(),
                        }))?;
                }
                self.prepare_container_start_inputs(pod_job, local_image.clone())?
            }
        };
        self.api
            .create_container(container_options, container_config)
            .await?;
        self.api
            .start_container(&container_name, None::<StartContainerOptions<String>>)
            .await?;
        PodRun::new(pod_job.clone(), self)
    }

    #[expect(
        clippy::cast_possible_wrap,
        clippy::cast_possible_truncation,
        reason = r#"
        - No issue in memory casting if between 0 - 2^63(i64:MAX, 8EB)
        - No issue in cores casting if in increments of 1e-9(nanocore)
        "#
    )]
    fn prepare_container_start_inputs(
        &self,
        pod_job: &PodJob,
        image: String,
    ) -> Result<(
        String,
        Option<CreateContainerOptions<String>>,
        Config<String>,
    )> {
        // Ensure output directory exists to prevent permissions issues if daemon's owner is root
        let host_output_directory = self
            .data_directory
            .join(pod_job.output_stream_path.location.clone());
        fs::create_dir_all(&host_output_directory)?;
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
        let unary_input_binds = pod_job
            .pod
            .input_stream_map
            .iter()
            .filter_map(|(stream_name, stream_info)| {
                match &pod_job.input_stream_path[stream_name] {
                    Input::Unary(blob) => Some(format!(
                        "{}:{}:{}",
                        self.data_directory
                            .join(blob.location.clone())
                            .to_string_lossy(),
                        stream_info.path.to_string_lossy(),
                        "ro"
                    )),
                    Input::Collection(_) => None,
                }
            })
            .collect::<Vec<_>>();
        let output_bind = [format!(
            "{}:{}",
            host_output_directory.to_string_lossy(),
            pod_job.pod.output_dir.to_string_lossy(),
        )];
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
        filters: HashMap<String, Vec<String>>,
    ) -> Result<impl Iterator<Item = RunInfo>> {
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
            Some(RunInfo {
                name: container_name,
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
                state: match (
                    container_spec.state.as_ref()?.status.as_ref()?,
                    container_spec.state.as_ref()?.exit_code? as i16,
                ) {
                    (ContainerStateStatusEnum::RUNNING, _) => RunState::Running,
                    (ContainerStateStatusEnum::EXITED, 0) => RunState::Completed,
                    (ContainerStateStatusEnum::EXITED, code) => RunState::Failed(code),
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
                cpu_limit: container_spec.host_config.as_ref()?.nano_cpus? as f32 / 10_f32.powi(9), // ncpu, ucores=3, mcores=6, cores=9
                memory_limit: container_spec.host_config.as_ref()?.memory? as u64,
            })
        }))
    }
}
