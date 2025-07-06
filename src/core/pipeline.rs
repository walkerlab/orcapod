use crate::uniffi::{error::Result, model::PipelineJob, orchestrator::agent::AgentClient};
use std::{collections::HashMap, path::PathBuf, sync::Arc};

#[expect(unused_variables, reason = "debug")]
pub async fn process_pipeline_job(
    agent_client: Arc<AgentClient>,
    status_topic: String,
    dispatch_topic: &str,
    pipeline_job: PipelineJob,
    namespace_lookup: HashMap<String, PathBuf>,
) -> Result<()> {
    Ok(())
}
