use super::PipelineRun;
use crate::{
    core::{
        crypto::{hash_buffer, hash_stream},
        model::serialize_hashmap,
        util::get,
    },
    uniffi::{
        error::{OrcaError, Result, selector},
        model::{PathSet, Pod, PodJob, URI},
        pipeline::{Kernel, Node, PipelineJob, PipelineResult},
    },
};
use futures_util::stream::FuturesUnordered;
use itertools::Itertools;
use serde_yaml::Serializer;
use snafu::OptionExt as _;
use std::{
    clone,
    collections::HashMap,
    mem,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{
    sync::broadcast::{self, Receiver, Sender},
    task::JoinSet,
};
use tokio_stream::StreamExt as _;

#[derive(Clone, Debug)]
pub(crate) enum Message {
    NodeOutput(String, HashMap<String, PathSet>), // String is the parent_node_name, while HashMap is output of the parent node
    Stop,                                         // Message to halt all operations
}

struct PipelineRunInfo {
    node_task_join_set: JoinSet<Result<()>>, // Join set to track the tasks for this pipeline run
    job_manager_ch_tx: Sender<Message>,
    node_tx: HashMap<String, Sender<Message>>,
    outputs: HashMap<String, HashMap<String, PathSet>>, // String is the node key, while hash
    namespace_lookup: HashMap<String, String>,          // Namespace to operate as storage
}

/// Docker based pipeline runner meant to execute on a single machine
#[derive(Default)]
pub struct DockerPipelineRunner {
    pipeline_runs: HashMap<PipelineRun, PipelineRunInfo>, // For each pipeline run, we have a join set to track the tasks and wait on them
}

impl DockerPipelineRunner {
    /// Create a new Docker pipeline runner
    pub fn new() -> Self {
        Self::default()
    }

    /// Start the `pipeline_job` returning `pipeline_run`un
    ///
    /// # Errors
    /// Will error out if the pipeline job fails to start
    pub fn start(
        &mut self,
        pipeline_job: PipelineJob,
        namespace_lookup: HashMap<String, String>,
    ) -> Result<PipelineRun> {
        // Create a new pipeline run
        let pipeline_run = PipelineRun { pipeline_job };
        let pipeline_run_arc = Arc::new(pipeline_run.clone());

        // Insert into the list of pipeline runs
        self.pipeline_runs.insert(
            (*pipeline_run_arc).clone(),
            PipelineRunInfo {
                job_manager_ch_tx: broadcast::channel::<Message>(1).0,
                node_tx: HashMap::new(),
                node_task_join_set: JoinSet::new(),
                outputs: HashMap::new(),
                namespace_lookup,
            },
        );

        // Create the source channel for the pipeline
        // This channel will be used to send inputs to the pipeline
        let (source_tx, _) = broadcast::channel::<Message>(1);

        // Get reference to the pipeline
        let pipeline = &pipeline_run_arc.pipeline_job.pipeline;

        // Get all the leaf nodes and call the create_task_for_node function for each leaf node
        // This will recursively create all the tasks and channels for the pipeline
        pipeline.get_leaf_nodes().try_for_each(|node| {
            self.create_task_for_node(node, &pipeline_run_arc, &source_tx)?;
            Ok::<(), OrcaError>(())
        })?;

        for node_key in pipeline.get_leaf_nodes() {
            self.create_task_for_node(node_key, &pipeline_run_arc, &source_tx)?;
        }

        // Create a task to handle outputs of output nodes in pipeline
        // for node_key in pipeline.output_nodes {}

        Ok(pipeline_run)
    }

    /// Given a pipeline run, wait for all its tasks to complete and return the `PipelineResult`
    ///
    /// # Errors
    /// Will error out if any of the pipeline tasks failed to join
    pub async fn get_result(&mut self, pipeline_run: &PipelineRun) -> Result<PipelineResult> {
        // Call join on the join set for the pipeline run
        let pipeline_run_info =
            self.pipeline_runs
                .get_mut(pipeline_run)
                .context(selector::KeyMissing {
                    key: pipeline_run.to_string(),
                })?;

        // Wait for all the tasks to complete
        while let Some(result) = pipeline_run_info.node_task_join_set.join_next().await {
            match result {
                Ok(Ok(())) => {} // Task completed successfully
                Ok(Err(err)) => {
                    eprintln!("Task failed: {err}");
                    return Err(err);
                }
                Err(err) => {
                    eprintln!("Join set error: {err}");
                    return Err(err.into());
                }
            }
        }

        Ok(PipelineResult {
            pipeline_job: pipeline_run.pipeline_job.clone(),
        })
    }

    fn create_task_for_node(
        &mut self,
        node: &Node,
        pipeline_run: &Arc<PipelineRun>,
        source_tx: &Sender<Message>,
    ) -> Result<Sender<Message>> {
        // Get the input channels for this node which should be it's parents
        let mut input_ch_rxs = pipeline_run
            .pipeline_job
            .pipeline
            .get_parents_for_node(node)
            .map(|parent_node| {
                // Check if it exists in the pipeline_runs hashmap
                match get(&self.pipeline_runs, pipeline_run)?
                    .node_tx
                    .get(&parent_node.id)
                {
                    Some(rx) => Ok(rx.subscribe()),
                    None => {
                        // Missing parent node, thus call create_task for the parent node parent node first
                        Ok(self
                            .create_task_for_node(parent_node, pipeline_run, source_tx)?
                            .subscribe())
                    }
                }
            })
            .collect::<Result<Vec<_>>>()?;

        // Check if input_ch_rxs is empty, meaning this node has no parents and is a root node
        // In this case, we will use the source channel as the input channel
        // TODO: This will be replaced by input_node logic once that is merged
        if input_ch_rxs.is_empty() {
            // No parents, thus this is root node
            // The parent rx will be the source channel rx
            input_ch_rxs.push(source_tx.subscribe());
        }

        // Get the job manager ch and subscribe to it (mainly for receiving shutdown signal)
        let job_manager_ch_rx = get(&self.pipeline_runs, pipeline_run)?
            .job_manager_ch_tx
            .subscribe();

        // Create the output_channel for this node
        let (tx, _) = broadcast::channel::<Message>(128);

        // Spawn the node_manager for this node
        self.pipeline_runs
            .get_mut(pipeline_run)
            .context(selector::KeyMissing {
                key: pipeline_run.to_string(),
            })?
            .node_task_join_set
            .spawn(Self::start_node_manager(
                node.clone(),
                Arc::clone(pipeline_run),
                input_ch_rxs,
                job_manager_ch_rx,
                tx.clone(),
            ));

        // Insert it into the the tx into the pipeline_runs hashmap
        self.pipeline_runs
            .get_mut(pipeline_run)
            .context(selector::KeyMissing {
                key: pipeline_run.to_string(),
            })?
            .node_tx
            .insert(node.id.clone(), tx.clone());

        // Return tx
        Ok(tx)
    }

    /// For tx: Sender<Message>, we only want to send successfully completed results to the next node
    async fn start_node_manager(
        node: Node,
        pipeline_run: Arc<PipelineRun>,
        parent_channel_rxs: Vec<Receiver<Message>>,
        mut job_manager_channel: Receiver<Message>,
        tx: Sender<Message>,
        namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<()> {
        // Create a futures unordered set to dynamically listen to N number of receivers
        let mut futures = FuturesUnordered::new();

        // Add all the parent channel receivers to the futures unordered set
        for mut rx in parent_channel_rxs {
            futures.push(tokio::spawn(async move { rx.recv().await }));
        }

        // Add the job manager channel to the futures unordered set
        futures.push(tokio::spawn(
            async move { job_manager_channel.recv().await },
        ));

        // Get the kernel for this node
        let kernel = get(
            &pipeline_run.pipeline_job.pipeline.kernel_lut,
            &node.kernel_hash,
        )?;

        // Set up a join_set to track the tasks ()
        let mut task_join_set = JoinSet::new();

        // Listen to the MPSC channel and handle messages
        while let Some(result) = futures.next().await {
            let rx_result = match result {
                Ok(rx_result) => rx_result,
                Err(err) => {
                    // Record into pipeilne_error log
                    if err.is_panic() {
                        eprintln!("Task panicked: {err}");
                    } else {
                        eprintln!("Error receiving message: {err}");
                    }
                    continue;
                }
            };

            let Ok(msg) = rx_result else {
                eprintln!("Failed to receive message from parent channel");
                continue;
            };

            match msg {
                Message::NodeOutput(_, input_packet) => {
                    // Inputs from parents are ready, thus we need to process them if they are already computed and cached
                    // NOTE: Cache is TODO
                    match kernel {
                        Kernel::Pod(pod) => {
                            Self::process_packet_pod(
                                &node,
                                pod.clone(),
                                tx.clone(),
                                tx.clone(),
                                input_packet,
                                Arc::clone(&pipeline_run),
                                namespace_lookup,
                            )?;
                        }
                        Kernel::Mapper(mapper) => {
                            // For mapper, we just apply it directly
                            let output_map = mapper
                                .mapping
                                .iter()
                                .map(|(input_key, output_key)| {
                                    let input = get(&input_packet, input_key)?.clone();
                                    Ok((output_key.to_owned(), input))
                                })
                                .collect::<Result<HashMap<_, _>>>()?;

                            // Send the output via the channel
                            tx.send(Message::NodeOutput(node_key.clone(), output_map))?;
                        }
                        Kernel::Joiner(joiner) => todo!(),
                    }
                }
                Message::Stop => {
                    // Stop all pod_job tasks abruptly
                    task_join_set.shutdown().await;
                    break;
                }
            }
        }

        Ok(())
    }

    fn process_packet_pod(
        node: &Node,
        pod: Arc<Pod>,
        success_ch_tx: Sender<Message>,
        failure_ch_tx: Sender<Message>,
        input_packet: HashMap<String, PathSet>,
        pipeline_run: Arc<PipelineRun>,
        namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<()> {
        // Output directory is pod_runs/pod_run_id/node_id/hash_of_input_packet

        // Compute the hash of the input_packet
        let mut buf = Vec::new();
        let mut serializer = Serializer::new(&mut buf);
        serialize_hashmap(&input_packet, &mut serializer)?;
        let input_packet_hash = hash_buffer(buf);
        let output_dir = URI {
            namespace: pipeline_run.pipeline_job.output_dir.namespace.clone(),
            path: PathBuf::from(format!("pod_runs/{}/{}", pod.hash, input_packet_hash)),
        };

        let cpu_limit = pod.recommended_cpus;
        let memory_limit = pod.recommended_memory;

        // Create the pod job
        let pod_job = PodJob::new(
            None,
            pod,
            input_packet.clone(),
            output_dir,
            cpu_limit,
            memory_limit,
            None,
            namespace_lookup,
        )?;

        // Simulate pod execution by just printing out pod_job_hash and pod hash
        // This will be replaced by sending the pod_job to the orchestrator via the agent
        println!(
            "Executing pod job: {} with pod hash: {}",
            pod_job.hash, pod_job.pod.hash
        );

        // For now we will just send the input_packet to the success channel
        success_ch_tx.send(Message::NodeOutput(node.id.clone(), input_packet.clone()))?;

        Ok(())
    }
}

trait ProcessPacket {
    fn process_packet(
        &mut self,
        sender_node_id: String,
        packet: HashMap<String, PathSet>,
        success_ch_tx: Sender<Message>,
        failure_ch_tx: Sender<Message>,
    ) -> Result<()>;
}

struct PodNodeProcessor {}

struct MapperProcessor {}

struct JoinNodeProcessor {
    /// Cache for all packets received by the node
    input_packet_cache: HashMap<String, Vec<HashMap<String, PathSet>>>,
}

impl JoinNodeProcessor {
    fn new(self, parents_node_id: Vec<String>) -> Self {
        let input_packet_cache = parents_node_id
            .into_iter()
            .map(|id| (id, Vec::new()))
            .collect();
        Self { input_packet_cache }
    }

    fn compute_new_packet_combination(
        &self,
        sender_node_id: String,
        new_packet: &HashMap<String, PathSet>,
    ) -> Result<Vec<HashMap<String, PathSet>>> {
        // Combine the new packet with the existing packets in the cache
        // Get all the cached packets from other parents
        let other_parent_ids = self
            .input_packet_cache
            .keys()
            .filter(|key| *key != &sender_node_id);
        let mut factors = other_parent_ids
            .map(|id| get(&self.input_packet_cache, id))
            .collect::<Result<Vec<_>>>()?;

        // Add the new incoming packet as a factor
        let incoming_packet = vec![new_packet.clone()];
        factors.push(&incoming_packet);

        let result = factors
            .into_iter()
            .multi_cartesian_product()
            .map(|packets_to_combined| {
                packets_to_combined
                    .into_iter()
                    .fold(HashMap::new(), |mut acc, packet| {
                        acc.extend(packet.clone());
                        acc
                    })
            })
            .collect::<Vec<_>>();

        Ok(result)
    }
}

impl ProcessPacket for JoinNodeProcessor {
    fn process_packet(
        &mut self,
        sender_node_id: String,
        packet: HashMap<String, PathSet>,
        success_ch_tx: Sender<Message>,
        failure_ch_tx: Sender<Message>,
    ) -> Result<()> {
        match {
            get(&self.input_packet_cache, &sender_node_id)?.push(packet);

            // Compute the new packet combination based on the sender node id and the packet
            let new_packets_to_send =
                self.compute_new_packet_combination(sender_node_id, &packet)?;

            Ok::<Vec<HashMap<String, PathSet>>, OrcaError>(new_packets_to_send)
        } {
            Ok(output_packets) => {
                // Send the output packets to the success channel
                for output_packet in output_packets {
                    success_ch_tx
                        .send(Message::NodeOutput(sender_node_id.clone(), output_packet))?;
                }
            }
            Err(err) => {
                // Send the error to the failure channel
                failure_ch_tx.send(Message::NodeOutput(
                    sender_node_id.clone(),
                    HashMap::new(), // Empty packet on failure
                ))?;
                return Err(err);
            }
        }
        // Add the new packet into the cache

        Ok(())
    }
}
