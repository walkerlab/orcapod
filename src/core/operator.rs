use crate::{
    core::{crypto::hash_buffer, model::serialize_hashmap},
    uniffi::{
        error::{Kind, OrcaError, Result, selector},
        model::{
            packet::{Packet, URI},
            pod::{Pod, PodJob, PodResult, PodResultStatus},
        },
        orchestrator::agent::{AgentClient, Response},
    },
};
use itertools::Itertools as _;
use serde_yaml::Serializer;
use snafu::{OptionExt as _, ResultExt as _};
use std::{
    clone::Clone as _, collections::HashMap, iter::IntoIterator as _, path::PathBuf, sync::Arc,
};
use tokio::{sync::Mutex, task::JoinSet};

#[allow(async_fn_in_trait, reason = "We only use this internally")]
pub trait Operator {
    async fn process_packets(&self, packets: Vec<(String, Packet)>) -> Result<Vec<Packet>>;
}

pub struct PodOperator {
    node_id: String,
    pod: Arc<Pod>,
    namespace: String,
    namespace_lookup: Arc<HashMap<String, PathBuf>>,
    client: Arc<AgentClient>,
}

impl PodOperator {
    pub const fn new(
        node_id: String,
        pod: Arc<Pod>,
        namespace: String,
        namespace_lookup: Arc<HashMap<String, PathBuf>>,
        client: Arc<AgentClient>,
    ) -> Self {
        Self {
            node_id,
            pod,
            namespace,
            namespace_lookup,
            client,
        }
    }

    async fn process_packet(
        pod_job_hash: String,
        node_id: String,
        packet: Packet,
        pod: Arc<Pod>,
        namespace: String,
        namespace_lookup: Arc<HashMap<String, PathBuf>>,
        client: Arc<AgentClient>,
    ) -> Result<Vec<Packet>> {
        let input_packet_hash = {
            let mut buf = Vec::new();
            let mut serializer = Serializer::new(&mut buf);
            serialize_hashmap(&packet, &mut serializer)?;
            hash_buffer(buf)
        };

        // Create the pod job
        let pod_job = PodJob::new(
            None,
            Arc::clone(&pod),
            packet,
            URI::new(
                namespace,
                format!("pod_runs/{pod_job_hash}/{node_id}/{input_packet_hash}").into(),
            ),
            pod.recommended_cpus,
            pod.recommended_memory,
            None,
            &namespace_lookup,
        )?;

        // Create listener for pod_job
        let target_key_exp = format!("group/{}/pod_job/{}/**", client.group, pod_job.hash);
        // Create the subscriber
        let pod_job_subscriber = client
            .session
            .declare_subscriber(target_key_exp)
            .await
            .context(selector::AgentCommunicationFailure {})?;

        // Create the async task to listen for the pod job completion
        let pod_job_listener_task = tokio::spawn(async move {
            // Wait for the pod job to complete and extract the result
            let sample = pod_job_subscriber
                .recv_async()
                .await
                .context(selector::AgentCommunicationFailure {})?;
            // Extract the pod_result from the payload
            let pod_result: PodResult = serde_json::from_slice(&sample.payload().to_bytes())?;
            Ok::<_, OrcaError>(pod_result)
        });

        // Submit it to the client and get the response to make sure it was successful
        let responses = client.start_pod_jobs(vec![pod_job.clone().into()]).await;
        let response = responses
            .first()
            .context(selector::InvalidIndex { idx: 0_usize })?;

        match response {
            Response::Ok => (),
            Response::Err(err) => {
                return Err(OrcaError {
                    kind: Kind::PodJobProcessingError {
                        hash: pod_job.hash,
                        reason: err.clone(),
                        backtrace: Some(snafu::Backtrace::capture()),
                    },
                });
            }
        }

        // Get the pod result from the listener task
        let pod_result = pod_job_listener_task.await??;

        // Get the output packet for the pod result
        let output_packet = match pod_result.status {
            PodResultStatus::Completed => {
                // Get the output packet
                pod_result.output_packet
            }
            PodResultStatus::Failed(exit_code) => {
                // Processing failed, thus return the error
                return Err(OrcaError {
                    kind: Kind::PodJobProcessingError {
                        hash: pod_result.pod_job.hash.clone(),
                        reason: format!("Pod processing failed with exit code {exit_code}"),
                        backtrace: Some(snafu::Backtrace::capture()),
                    },
                });
            }
            PodResultStatus::Unset => {
                // This should not happen, but if it does, we will return an error
                return Err(OrcaError {
                    kind: Kind::PodJobProcessingError {
                        hash: pod_result.pod_job.hash.clone(),
                        reason: "Pod processing status is unset".to_owned(),
                        backtrace: Some(snafu::Backtrace::capture()),
                    },
                });
            }
        };

        Ok(vec![output_packet])
    }
}

impl Operator for PodOperator {
    async fn process_packets(&self, packets: Vec<(String, Packet)>) -> Result<Vec<Packet>> {
        let mut processing_tasks = JoinSet::new();

        for (pod_job_hash, packet) in packets {
            processing_tasks.spawn(Self::process_packet(
                pod_job_hash,
                self.node_id.clone(),
                packet,
                Arc::clone(&self.pod),
                self.namespace.clone(),
                Arc::clone(&self.namespace_lookup),
                Arc::clone(&self.client),
            ));
        }

        let mut new_packets = vec![];
        while let Some(result) = processing_tasks.join_next().await {
            match result {
                Ok(Ok(products)) => new_packets.extend(products),
                Ok(Err(err)) => return Err(err),
                Err(err) => return Err(err.into()),
            }
        }

        Ok(new_packets)
    }
}

pub struct JoinOperator {
    parent_count: usize,
    packet_cache: Arc<Mutex<HashMap<String, Vec<Packet>>>>,
}

impl JoinOperator {
    pub fn new(parent_count: usize) -> Self {
        Self {
            parent_count,
            packet_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn process_packet(
        parent_count: usize,
        packet_cache: Arc<Mutex<HashMap<String, Vec<Packet>>>>,
        parent_id: String,
        packet: Packet,
    ) -> Result<Vec<Packet>> {
        let mut packet_cache_lock = packet_cache.lock().await;
        packet_cache_lock
            .entry(parent_id.clone())
            .or_insert_with(|| vec![packet.clone()])
            .push(packet.clone());

        // If we still don't have at least 1 packet for each parent, skip computation
        if packet_cache_lock.len() < parent_count {
            return Ok(vec![]);
        }

        // Build the factors for the cartesian product, silently missing key since it shouldn't be possible
        let factors = packet_cache_lock
            .iter()
            .filter_map(|(id, parent_packets)| (*id != parent_id).then_some(parent_packets.clone()))
            .chain(vec![vec![packet]])
            .collect::<Vec<_>>();

        // We don't need the lock after this point
        drop(packet_cache_lock);

        // Compute the cartesian product of the factors, this might take a while
        Ok(factors
            .into_iter()
            .multi_cartesian_product()
            .map(|packets_to_combined| {
                packets_to_combined
                    .into_iter()
                    .fold(HashMap::new(), |mut acc, new_packet| {
                        acc.extend(new_packet);
                        acc
                    })
            })
            .collect::<Vec<_>>())
    }
}

impl Operator for JoinOperator {
    async fn process_packets(&self, packets: Vec<(String, Packet)>) -> Result<Vec<Packet>> {
        let mut processing_task = JoinSet::new();

        for (parent_id, packet) in packets {
            processing_task.spawn(Self::process_packet(
                self.parent_count,
                Arc::clone(&self.packet_cache),
                parent_id,
                packet,
            ));
        }

        let mut new_packets = vec![];
        while let Some(result) = processing_task.join_next().await {
            match result {
                Ok(Ok(products)) => new_packets.extend(products),
                Ok(Err(err)) => return Err(err),
                Err(err) => return Err(err.into()),
            }
        }

        Ok(new_packets)
    }
}

pub struct MapOperator {
    map: HashMap<String, String>,
}

impl MapOperator {
    pub const fn new(map: HashMap<String, String>) -> Self {
        Self { map }
    }
}

impl Operator for MapOperator {
    async fn process_packets(&self, packets: Vec<(String, Packet)>) -> Result<Vec<Packet>> {
        Ok(packets
            .into_iter()
            .map(|(_, packet)| {
                packet
                    .iter()
                    .map(|(packet_key, path_set)| {
                        (
                            self.map
                                .get(packet_key)
                                .map_or_else(|| packet_key.clone(), Clone::clone),
                            path_set.clone(),
                        )
                    })
                    .collect()
            })
            .collect::<Vec<_>>())
    }
}
