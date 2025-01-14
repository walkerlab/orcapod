// use crate::error::Result;
use std::error::Error;

#[expect(dead_code, reason = "debug")]
#[derive(Debug)]
pub(crate) struct ContainerInfo {
    name: String,
    image: String,
    created: i64,
    entrypoint: String,
    command: String,
    state: String,
    mounts: Vec<String>,
    nano_cpu_limit: i64,
    memory_limit: i64,
}

/// Standard behavior of any container orchestration engine supported.
pub trait Orchestrator {
    /// How to start containers.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue querying metadata from containers.
    fn start(&self) -> Result<(), Box<dyn Error>>;
    /// How to start containers with alternate image.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue querying metadata from containers.
    // fn start_with_altimage(&self) -> Result<(), Box<dyn Error>>;
    /// How to load an image.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue querying metadata from containers.
    // fn load(&self) -> Result<(), Box<dyn Error>>;
    /// How to delete containers.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue querying metadata from containers.
    fn delete(&self) -> Result<(), Box<dyn Error>>;
    /// How to get container logs.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue querying metadata from containers.
    // fn get_logs(p: &PodRun) -> Result<String, OrchestrationError>;
    /// How to get resource usage for container.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue querying metadata from containers.
    // fn metrics() -> OrchestratorMetrics;
    /// How to query containers.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue querying metadata from containers.
    fn list(&self) -> Result<(), Box<dyn Error>>;
}

// struct OrchestratorMetrics {
//     // represents orchestrator resource usage info
//     // would be cool if this would be a handle to info stream
// }

// struct OrchestrationError {}

/// Orchestration implementation for Docker backend.
pub mod docker;
