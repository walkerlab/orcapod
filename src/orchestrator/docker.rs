use crate::{
    error::{Kind, OrcaError, Result},
    model::{Input, PodJob},
    orchestrator::{self, PodRun, PodRunAPI, RunInfo, RunState, Types},
    store::ModelID,
};
use bollard::{
    container::{
        Config, CreateContainerOptions, ListContainersOptions, RemoveContainerOptions,
        StartContainerOptions, WaitContainerOptions,
    },
    models::{ContainerStateStatusEnum, HostConfig},
    Docker,
};
use futures::{future::join_all, stream::TryStreamExt};
use names::{Generator, Name};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use tokio::runtime::Runtime;

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
    fn get_info(&self) -> Result<Option<RunInfo>> {
        let mut labels = match &self.pod_job_model_id {
            ModelID::Hash(hash) => {
                vec![
                    "org.orcapod.pod_job.model_id.type=hash".to_owned(),
                    format!("org.orcapod.pod_job.model_id.hash={hash}"),
                ]
            }
            ModelID::Annotation(name, version) => {
                vec![
                    "org.orcapod.pod_job.model_id.type=annotation".to_owned(),
                    format!("org.orcapod.pod_job.model_id.name={name}"),
                    format!("org.orcapod.pod_job.model_id.version={version}"),
                ]
            }
        };
        labels.push("org.orcapod=true".to_owned());
        Ok(self
            .orchestrator
            .async_driver
            .block_on(
                self.orchestrator
                    .list_containers(HashMap::from([("label".to_owned(), labels)])),
            )?
            .next())
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
                PodRun::new(
                    match run_info.labels["org.orcapod.pod_job.model_id.type"].as_ref() {
                        "annotation" => ModelID::Annotation(
                            run_info.labels["org.orcapod.pod_job.model_id.name"].clone(),
                            run_info.labels["org.orcapod.pod_job.model_id.version"].clone(),
                        ),
                        "hash" => ModelID::Hash(
                            run_info.labels["org.orcapod.pod_job.model_id.hash"].clone(),
                        ),
                        _ => todo!(),
                    },
                    self,
                )
            })
            .collect()
    }
    fn start(&self, pod_job: &PodJob) -> Result<impl PodRunAPI> {
        let (container_name, options, config) = self.prepare_container_start_inputs(pod_job)?;
        self.async_driver.block_on(async {
            self.api.create_container(options, config).await?;
            self.api
                .start_container(&container_name, None::<StartContainerOptions<String>>)
                .await?;
            self.api
                .wait_container(&container_name, None::<WaitContainerOptions<String>>)
                .try_collect::<Vec<_>>()
                .await
        })?;
        pod_job.annotation.as_ref().map_or_else(
            || PodRun::new(ModelID::Hash(pod_job.hash.clone()), self),
            |annotation| {
                PodRun::new(
                    ModelID::Annotation(annotation.name.clone(), annotation.version.clone()),
                    self,
                )
            },
        )
    }
    fn delete(&self, pod_run: &impl PodRunAPI) -> Result<()> {
        if let Some(run_info) = pod_run.get_info()? {
            self.async_driver.block_on(self.api.remove_container(
                &run_info.name,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            ))?;
        }
        Ok(())
    }
}

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
        let mut labels = pod_job.annotation.as_ref().map_or_else(
            || {
                HashMap::from([
                    (
                        "org.orcapod.pod_job.model_id.type".to_owned(),
                        "hash".to_owned(),
                    ),
                    (
                        "org.orcapod.pod_job.model_id.hash".to_owned(),
                        pod_job.hash.clone(),
                    ),
                ])
            },
            |annotation| {
                HashMap::from([
                    (
                        "org.orcapod.pod_job.model_id.type".to_owned(),
                        "annotation".to_owned(),
                    ),
                    (
                        "org.orcapod.pod_job.model_id.name".to_owned(),
                        annotation.name.clone(),
                    ),
                    (
                        "org.orcapod.pod_job.model_id.version".to_owned(),
                        annotation.version.clone(),
                    ),
                ])
            },
        );
        labels.insert("org.orcapod".to_owned(), "true".to_owned());
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
                image: Some(pod_job.pod.image.clone()),
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
            Some(RunInfo {
                name: container_name,
                image: container_spec.config.as_ref()?.image.as_ref()?.clone(),
                created: container_summary.created? as u64,
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
