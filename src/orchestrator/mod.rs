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
    /// How to delete containers.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue querying metadata from containers.
    fn delete(&self) -> Result<(), Box<dyn Error>>;
    // fn get_logs(p: &PodRun) -> Result<String, OrchestrationError>;
    // fn list() -> Result<BTreeMap<String, Vec<String>>>;
    /// How to query containers.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue querying metadata from containers.
    fn list(&self) -> Result<(), Box<dyn Error>>;
    // fn metrics() -> OrchestratorMetrics;
}

// struct OrchestratorMetrics {
//     // represents orchestrator resource usage info
//     // would be cool if this would be a handle to info stream
// }

// struct OrchestrationError {}

/// Orchestration implementation for Docker backend.
pub mod docker;
