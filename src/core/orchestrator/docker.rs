use crate::{
    core::util::get,
    uniffi::{
        error::{Result, selector},
        model::{Input, PodJob},
        orchestrator::{RunInfo, Status, docker::LocalDockerOrchestrator},
    },
};
use bollard::{
    container::{Config, CreateContainerOptions, ListContainersOptions, RemoveContainerOptions},
    models::{ContainerStateStatusEnum, HostConfig},
    secret::{ContainerInspectResponse, ContainerSummary},
};
use chrono::DateTime;
use futures_util::future::join_all;
use names::{Generator, Name};
use regex::Regex;
use snafu::OptionExt as _;
use std::{
    collections::HashMap,
    fs,
    path::{self, PathBuf},
    sync::LazyLock,
};

#[expect(clippy::expect_used, reason = "Valid static regex")]
pub static RE_IMAGE_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
                \s
                    (?<image>[^\s]+:[^\s]+)
                \s
            ",
    )
    .expect("Invalid image tag regex.")
});

#[expect(clippy::expect_used, reason = "Valid static regex")]
static RE_FOR_CMD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"[^\s"']+|"[^"]*"|'[^']*'"#).expect("Invalid model metadata regex.")
});

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
        let input_binds = pod_job
            .pod
            .input_stream
            .iter()
            .try_fold::<_, _, Result<_>>(
                vec![],
                |mut flattened_binds, (stream_name, stream_info)| {
                    flattened_binds.extend(match get(&pod_job.input_map, stream_name)? {
                        Input::Unary(blob) => {
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
                        Input::Collection(blobs) => blobs
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
                                            selector::NoFileName {
                                                path: blob.location.path.clone()
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
    /// Prepare the inputs for starting a container.
    ///
    /// # Errors
    /// Will fail if pod job is invalid
    pub fn prepare_container_start_inputs(
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
        let container_name = Generator::with_naming(Name::Plain)
            .next()
            .context(selector::GeneratedNamesOverflow)?;
        let labels = HashMap::from([
            ("org.orcapod".to_owned(), "true".to_owned()),
            (
                "org.orcapod.pod.annotation".to_owned(),
                serde_json::to_string(&pod_job.pod.annotation)?,
            ),
            ("org.orcapod.pod.hash".to_owned(), pod_job.pod.hash.clone()),
            (
                "org.orcapod.pod".to_owned(),
                serde_json::to_string(&*pod_job.pod)?,
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
        let command = RE_FOR_CMD
            .captures_iter(&pod_job.pod.command)
            .map(|capture| {
                capture
                    .extract::<0>()
                    .0
                    .to_owned()
                    .replace(['\'', '\"'], "")
            })
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
    ) -> Result<impl Iterator<Item = Result<(String, RunInfo)>>> {
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
                        .context(selector::NoContainerNames)?[0][1..];
                    Ok((
                        container_name.to_owned(),
                        container_summary.clone(),
                        self.api.inspect_container(container_name, None).await?,
                    ))
                }),
        )
        .await
        .into_iter()
        .map(|result: Result<_>| {
            let (container_name, container_summary, container_inspect_response) = match result {
                Ok((container_name, container_summary, container_inspect_response)) => (
                    container_name,
                    container_summary,
                    container_inspect_response,
                ),
                Err(error) => {
                    return Err(error);
                }
            };

            Ok(
                Self::extract_run_info(&container_summary, &container_inspect_response)
                    .map(|run_info| (container_name.clone(), run_info))
                    .context(selector::FailedToExtractRunInfo { container_name })?,
            )
        }))
    }
    pub(crate) async fn delete_container(&self, container_name: &str) -> Result<()> {
        self.api
            .remove_container(
                container_name,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await?;
        Ok(())
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
    ) -> Option<RunInfo> {
        let terminated_timestamp = DateTime::parse_from_rfc3339(
            container_inspect_response
                .state
                .as_ref()?
                .finished_at
                .as_ref()?,
        )
        .ok()?
        .timestamp() as u64;
        Some(RunInfo {
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
            command: format!(
                "{} {}",
                container_inspect_response
                    .config
                    .as_ref()?
                    .entrypoint
                    .as_ref()?
                    .join(" "),
                container_inspect_response
                    .config
                    .as_ref()?
                    .cmd
                    .as_ref()?
                    .join(" ")
            ),
            status: match (
                container_inspect_response.state.as_ref()?.status?,
                container_inspect_response.state.as_ref()?.exit_code? as i16,
            ) {
                (ContainerStateStatusEnum::RUNNING, _) => Status::Running,
                (ContainerStateStatusEnum::EXITED, 0) => Status::Completed,
                (ContainerStateStatusEnum::EXITED | ContainerStateStatusEnum::DEAD, code) => {
                    Status::Failed(code)
                }
                (
                    ContainerStateStatusEnum::CREATED | ContainerStateStatusEnum::RESTARTING,
                    code,
                ) => {
                    if container_inspect_response
                        .state
                        .as_ref()?
                        .error
                        .as_ref()?
                        .is_empty()
                    {
                        Status::Queued
                    } else {
                        Status::Failed(code)
                    }
                }
                _ => Status::Unknown,
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
