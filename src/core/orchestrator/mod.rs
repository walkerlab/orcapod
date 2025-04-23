use crate::{
    core::util::get_type_name,
    uniffi::{
        model::PodJob,
        orchestrator::{Orchestrator, PodRun},
    },
};

impl PodRun {
    pub(crate) fn new<O: Orchestrator>(pod_job: &PodJob, assigned_name: String) -> Self {
        Self {
            pod_job: pod_job.clone().into(),
            orchestrator_source: get_type_name::<O>(),
            assigned_name,
        }
    }
}

pub mod docker;
