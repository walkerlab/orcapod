// docker run --rm -d --entrypoint tail -v /tmp/stuff-outer:/tmp/stuff-inner:ro --cpus=0.5 --memory=500m alpine:3.14 -f /dev/null
use crate::orchestrator::{ContainerInfo, Orchestrator};
use bollard::{container::ListContainersOptions, Docker};
use std::{collections::HashMap, default::Default, error::Error};
use tokio::runtime::Runtime;

/// Support for an orchestration engine using a local docker installation.
pub struct LocalDockerOrchestrator;

impl Orchestrator for LocalDockerOrchestrator {
    // #[expect(clippy::string_slice, reason = "debug")]
    // fn list() -> Result<(), Box<dyn Error>> {
    //     let docker = Docker::connect_with_local_defaults()?;
    //     let filters = HashMap::<&str, Vec<&str>>::new();
    //     // filters.insert("health", vec!["unhealthy"]);
    //     let options = Some(ListContainersOptions {
    //         all: true,
    //         filters,
    //         ..Default::default()
    //     });
    //     // let containers = Runtime::new()?
    //     //     .block_on(docker.list_containers(options))?
    //     //     .into_iter()
    //     //     .map(|container_summary| {
    //     //         Ok(ContainerInfo {
    //     //             name: container_summary.names.ok_or("wow")?[0][1..].to_string(),
    //     //             image: container_summary.image.ok_or("wow")?,
    //     //             entrypoint_and_command: container_summary.command.ok_or("wow")?,
    //     //             created: container_summary.created.ok_or("wow")?,
    //     //             state: container_summary.state.ok_or("wow")?,
    //     //             mounts: container_summary
    //     //                 .mounts
    //     //                 .ok_or("wow")?
    //     //                 .into_iter()
    //     //                 .map(|mount_point| {
    //     //                     Ok(format!(
    //     //                         "{}:{}{}",
    //     //                         mount_point.source.ok_or("wow")?,
    //     //                         mount_point.destination.ok_or("wow")?,
    //     //                         if mount_point.mode.is_some() {
    //     //                             format!(":{}", mount_point.mode.ok_or("wow")?)
    //     //                         } else {
    //     //                             String::new()
    //     //                         },
    //     //                     ))
    //     //                 })
    //     //                 .collect::<Result<Vec<_>, Box<dyn Error>>>()?,
    //     //         })
    //     //     })
    //     //     .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    //     let containers = Runtime::new()?.block_on(docker.list_containers(options))?;
    //     println!("{containers:?}");
    //     Ok(())
    // }
    // #[expect(clippy::string_slice, reason = "debug")]
    // fn list() -> Result<(), Box<dyn Error>> {
    //     let docker = Docker::connect_with_local_defaults()?;
    //     let tokio_runtime = Runtime::new()?;
    //     let container_names = tokio_runtime
    //         .block_on(docker.list_containers(Some(ListContainersOptions {
    //             all: true,
    //             filters: HashMap::<&str, Vec<&str>>::new(),
    //             ..Default::default()
    //         })))?
    //         .into_iter()
    //         .map(|container_summary| Ok(container_summary.names.ok_or("wow")?[0][1..].to_string()))
    //         .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    //     println!("{container_names:?}");

    //     let container_spec = tokio_runtime.block_on(docker.inspect_container(
    //         "interesting_ritchie",
    //         Some(InspectContainerOptions { size: false }),
    //     ))?;
    //     println!("{container_spec:?}");
    //     Ok(())
    // }
    #[expect(clippy::string_slice, reason = "debug")]
    #[expect(clippy::use_debug, reason = "debug")]
    fn list() -> Result<(), Box<dyn Error>> {
        let docker = Docker::connect_with_local_defaults()?;
        let tokio_runtime = Runtime::new()?;
        let containers = tokio_runtime
            .block_on(docker.list_containers(Some(ListContainersOptions {
                all: true,
                filters: HashMap::<&str, Vec<&str>>::new(),
                ..Default::default()
            })))?
            .into_iter()
            .map(|container_summary| {
                let container_name =
                    container_summary.names.as_ref().ok_or("wow")?[0][1..].to_owned();
                let container_spec =
                    tokio_runtime.block_on(docker.inspect_container(&container_name, None))?;
                let container = ContainerInfo {
                    name: container_name,
                    image: container_spec
                        .config
                        .as_ref()
                        .ok_or("wow")?
                        .image
                        .as_ref()
                        .ok_or("wow")?
                        .clone(),
                    created: container_summary.created.ok_or("wow")?,
                    entrypoint: container_spec
                        .config
                        .as_ref()
                        .ok_or("wow")?
                        .entrypoint
                        .as_ref()
                        .ok_or("wow")?
                        .join(" "),
                    command: container_spec
                        .config
                        .as_ref()
                        .ok_or("wow")?
                        .cmd
                        .as_ref()
                        .ok_or("wow")?
                        .join(" "),
                    state: container_spec
                        .state
                        .as_ref()
                        .ok_or("wow")?
                        .status
                        .as_ref()
                        .ok_or("wow")?
                        .to_string(),
                    mounts: container_spec
                        .mounts
                        .as_ref()
                        .ok_or("wow")?
                        .iter()
                        .map(|mount_point| {
                            Ok(format!(
                                "{}:{}{}",
                                mount_point.source.as_ref().ok_or("wow")?,
                                mount_point.destination.as_ref().ok_or("wow")?,
                                if mount_point.mode.is_some() {
                                    format!(":{}", mount_point.mode.as_ref().ok_or("wow")?)
                                } else {
                                    String::new()
                                },
                            ))
                        })
                        .collect::<Result<Vec<_>, Box<dyn Error>>>()?,
                    nano_cpu_limit: container_spec
                        .host_config
                        .as_ref()
                        .ok_or("wow")?
                        .nano_cpus
                        .ok_or("wow")?,
                    memory_limit: container_spec
                        .host_config
                        .as_ref()
                        .ok_or("wow")?
                        .memory
                        .ok_or("wow")?,
                };
                // println!("{container_spec:?}");
                // println!("{container_summary:?}");
                Ok(container)
            })
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
        println!("{containers:?}");
        Ok(())
    }
}
