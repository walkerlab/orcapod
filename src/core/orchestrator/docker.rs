use crate::{
    core::util::get,
    uniffi::{
        error::{Result, selector},
        model::{packet::PathSet, pod::PodJob},
        orchestrator::{PodRunInfo, PodStatus, docker::LocalDockerOrchestrator},
    },
};
use bollard::{
    container::{Config, CreateContainerOptions, ListContainersOptions},
    models::{ContainerStateStatusEnum, HostConfig},
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
        let container_name = Generator::with_naming(Name::Plain)
            .next()
            .context(selector::GeneratedNamesOverflow)?;
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
                    binds: Some([&*input_binds, &output_bind].concat()),
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
        clippy::indexing_slicing,
        clippy::too_many_lines,
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
        .filter_map(|result: Result<_>| {
            let (container_name, container_summary, container_spec) = result.ok()?;
            let terminated_timestamp =
                DateTime::parse_from_rfc3339(container_spec.state.as_ref()?.finished_at.as_ref()?)
                    .ok()?
                    .timestamp();
            Some((
                container_name,
                PodRunInfo {
                    image: container_spec.config.as_ref()?.image.as_ref()?.clone(),
                    created: container_summary.created? as u64,
                    terminated: (terminated_timestamp > 0).then_some(terminated_timestamp as u64),
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
                        (ContainerStateStatusEnum::RUNNING, _) => PodStatus::Running,
                        (
                            ContainerStateStatusEnum::EXITED
                            | ContainerStateStatusEnum::REMOVING
                            | ContainerStateStatusEnum::DEAD,
                            0,
                        ) => PodStatus::Completed,
                        (
                            ContainerStateStatusEnum::EXITED
                            | ContainerStateStatusEnum::REMOVING
                            | ContainerStateStatusEnum::DEAD,
                            code,
                        ) => PodStatus::Failed(code),
                        (_, code) => {
                            todo!(
                                "Unhandled container state: {}, exit code: {code}.",
                                container_spec.state.as_ref()?.status.as_ref()?
                            )
                        }
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
                        .collect::<Option<_>>()?,
                    labels: container_spec.config.as_ref()?.labels.as_ref()?.clone(),
                    cpu_limit: container_spec.host_config.as_ref()?.nano_cpus? as f32
                        / 10_f32.powi(9), // ncpu, ucores=3, mcores=6, cores=9
                    memory_limit: container_spec.host_config.as_ref()?.memory? as u64,
                },
            ))
        }))
    }
}
