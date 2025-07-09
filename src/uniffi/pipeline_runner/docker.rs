use super::PipelineRun;
use crate::{
    core::{crypto::hash_buffer, model::serialize_hashmap, util::get},
    uniffi::{
        error::{OrcaError, Result, selector},
        model::{PathSet, Pod, PodJob, URI},
        pipeline::{Kernel, Mapper, Node, PipelineJob, PipelineResult},
    },
};
use futures_util::stream::FuturesUnordered;
use itertools::Itertools as _;
use serde_yaml::Serializer;
use snafu::OptionExt as _;
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::{
    sync::{
        broadcast::{self, Receiver, Sender, error::RecvError},
        oneshot,
    },
    task::{JoinHandle, JoinSet},
};
use tokio_stream::StreamExt as _;

#[derive(Clone, Debug)]
pub(crate) enum Message {
    /// String is the `parent_node_id`, while `HashMap` is output of the parent node
    NodeOutput(String, HashMap<String, PathSet>),
    /// String is the `node_id` that has completed processing
    NodeProcessingComplete(String),
    ProcessingFailed(String, Arc<OrcaError>), // String is the node_id, while OrcaError is the error that occurred
    Stop,                                     // Message to halt all operations
}

struct PipelineRunInfo {
    node_task_join_set: JoinSet<Result<()>>, // Join set to track the tasks for this pipeline run
    job_manager_ch_tx: Sender<Message>,
    node_tx: HashMap<String, Sender<Message>>,
    outputs: HashMap<String, HashMap<String, PathSet>>, // String is the node key, while hash
    namespace_lookup: HashMap<String, PathBuf>,
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
        namespace_lookup: &HashMap<String, PathBuf>,
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
                namespace_lookup: namespace_lookup.clone(),
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
            self.create_task_for_node(node, &pipeline_run_arc, &source_tx, namespace_lookup)?;
            Ok::<(), OrcaError>(())
        })?;

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
        namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<Sender<Message>> {
        println!("Creating task for node: {}", node.id);
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
                            .create_task_for_node(
                                parent_node,
                                pipeline_run,
                                source_tx,
                                namespace_lookup,
                            )?
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
                namespace_lookup.clone(),
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
        success_ch_tx: Sender<Message>,
        namespace_lookup: HashMap<String, PathBuf>,
    ) -> Result<()> {
        // Create a channel to for waiting when the node processing is complete
        let (node_complete_tx, node_complete_rx) = oneshot::channel::<()>();

        // Create a futures unordered set to dynamically listen to N number of receivers
        let chs_to_listen_to = FuturesUnordered::new();

        // Add all the parent channel receivers to the futures unordered set
        for mut rx in parent_channel_rxs {
            chs_to_listen_to.push(tokio::spawn(async move { rx.recv().await }));
        }

        // Add the job manager channel to the futures unordered set
        chs_to_listen_to.push(tokio::spawn(
            async move { job_manager_channel.recv().await },
        ));

        // Get the kernel for this node and build the correct processor
        match get(
            &pipeline_run.pipeline_job.pipeline.kernel_lut,
            &node.kernel_hash,
        )? {
            Kernel::Pod(pod) => PodNodeProcessor::new(
                Arc::clone(pod),
                node.id.clone(),
                chs_to_listen_to,
                success_ch_tx,
                pipeline_run.pipeline_job.output_dir.namespace.clone(),
                namespace_lookup,
                node_complete_tx,
            ),
            Kernel::Mapper(mapper) => {
                todo!()
            }
            Kernel::Joiner => {
                todo!()
            }
        };

        node_complete_rx.await;

        // // Listen to the MPSC channel and handle messages
        // while let Some(result) = chs_to_listen_to.next().await {
        //     let rx_result = match result {
        //         Ok(rx_result) => rx_result,
        //         Err(err) => {
        //             // Record into pipeline_error log
        //             if err.is_panic() {
        //                 eprintln!("Task panicked: {err}");
        //             } else {
        //                 eprintln!("Error receiving message: {err}");
        //             }
        //             continue;
        //         }
        //     };

        //     let Ok(msg) = rx_result else {
        //         eprintln!("Failed to receive message from parent channel");
        //         continue;
        //     };

        //     match msg {
        //         Message::NodeOutput(sender_node_id, packet) => {
        //             // Inputs from parents are ready, thus we need to process them if they are already computed and cached
        //             processor.process_packet(
        //                 &sender_node_id,
        //                 &node.id,
        //                 packet,
        //                 success_ch_tx.clone(),
        //                 failure_ch_tx.clone(),
        //             )?;
        //         }
        //         Message::Stop => {
        //             todo!()
        //         }
        //         Message::ProcessingFailed(_, orca_error) => todo!(),
        //         Message::NodeProcessingComplete(node_id) => ,
        //     }
        // }

        Ok(())
    }
}

struct PodNodeProcessor {
    pod: Arc<Pod>,
    node_id: String,
    ch_to_listen_to: FuturesUnordered<JoinHandle<Result<Message, RecvError>>>,
    success_ch_tx: Sender<Message>, // Channel to send successful outputs to the next node
    namespace: String,
    namespace_lookup: HashMap<String, PathBuf>, // Copy of the look up table
    node_complete_tx: oneshot::Sender<()>,
    processing_tasks: JoinSet<Result<(), OrcaError>>,
}

impl PodNodeProcessor {
    fn new(
        pod: Arc<Pod>,
        node_id: String,
        ch_to_listen_to: FuturesUnordered<JoinHandle<Result<Message, RecvError>>>,
        success_ch_tx: Sender<Message>,
        namespace: String,
        namespace_lookup: HashMap<String, PathBuf>,
        node_complete_tx: oneshot::Sender<()>,
    ) -> Self {
        Self {
            pod,
            node_id,
            ch_to_listen_to,
            success_ch_tx,
            namespace,
            namespace_lookup,
            node_complete_tx,
            processing_tasks: JoinSet::new(),
        }
    }

    async fn start(&mut self) {
        // Start to listen to the channels
        // Listen to the MPSC channel and handle messages

        while let Some(result) = self.ch_to_listen_to.next().await {
            let rx_result = match result {
                Ok(rx_result) => rx_result,
                Err(err) => {
                    // Record into pipeline_error log
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
                Message::NodeOutput(sender_node_id, packet) => {
                    let pod_ref = Arc::clone(&self.pod);
                    let node_id = self.node_id.clone();
                    let namespace = self.namespace.clone();
                    let namespace_lookup = self.namespace_lookup.clone();
                    let success_ch_tx = self.success_ch_tx.clone();
                    // Forward it into a processing task
                    self.processing_tasks.spawn(async move {
                        Self::process_packet(
                            &node_id,
                            &pod_ref,
                            &namespace,
                            &namespace_lookup,
                            &packet,
                            &success_ch_tx,
                        )
                    });
                }
                Message::Stop => {
                    todo!()
                }
                Message::ProcessingFailed(_, orca_error) => todo!(),
                Message::NodeProcessingComplete(node_id) => todo!(),
            }
        }
    }

    fn process_packet(
        node_id: &str,
        pod: &Arc<Pod>,
        namespace: &str,
        namespace_lookup: &HashMap<String, PathBuf>,
        packet: &HashMap<String, PathSet>,
        success_ch_tx: &Sender<Message>,
    ) -> Result<()> {
        // Process the packet using the pod

        // Create the pod_job
        let mut buf = Vec::new();
        let mut serializer = Serializer::new(&mut buf);
        serialize_hashmap(packet, &mut serializer)?;
        let input_packet_hash = hash_buffer(buf);
        let output_dir = URI {
            namespace: namespace.to_owned(),
            path: PathBuf::from(format!("pod_runs/{}/{}", pod.hash, input_packet_hash)),
        };

        let cpu_limit = pod.recommended_cpus;
        let memory_limit = pod.recommended_memory;

        // Create the pod job
        let pod_job = PodJob::new(
            None,
            Arc::clone(pod),
            packet.clone(),
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
        success_ch_tx.send(Message::NodeOutput(node_id.to_owned(), packet.clone()))?;

        Ok(())
    }
}

// struct MapperProcessor {
//     mapper: Arc<Mapper>,
// }

// impl NodeProcessor for MapperProcessor {
//     fn process_packet(
//         &mut self,
//         sender_node_id: String,
//         current_node_id: String,
//         packet: HashMap<String, PathSet>,
//         success_ch_tx: Sender<Message>,
//         _failure_ch_tx: Sender<Message>,
//     ) -> Result<()> {
//         // Apply the mapping to the input packet
//         let output_map = self
//             .mapper
//             .mapping
//             .iter()
//             .map(|(input_key, output_key)| {
//                 let input = get(&packet, input_key)?.clone();
//                 Ok((output_key.to_owned(), input))
//             })
//             .collect::<Result<HashMap<_, _>>>()?;

//         // Send the output via the channel
//         success_ch_tx.send(Message::NodeOutput(sender_node_id, output_map))?;
//         Ok(())
//     }
// }

// struct JoinerNodeProcessor {
//     /// Cache for all packets received by the node
//     input_packet_cache: HashMap<String, Vec<HashMap<String, PathSet>>>,
// }

// impl JoinerNodeProcessor {
//     fn new(parents_node_id: Vec<String>) -> Self {
//         let input_packet_cache = parents_node_id
//             .into_iter()
//             .map(|id| (id, Vec::new()))
//             .collect();
//         Self { input_packet_cache }
//     }

//     fn compute_new_packet_combination(
//         &self,
//         sender_node_id: &str,
//         new_packet: &HashMap<String, PathSet>,
//     ) -> Result<Vec<HashMap<String, PathSet>>> {
//         // Combine the new packet with the existing packets in the cache
//         // Get all the cached packets from other parents
//         let other_parent_ids = self
//             .input_packet_cache
//             .keys()
//             .filter(|key| *key != sender_node_id);
//         let mut factors = other_parent_ids
//             .map(|id| get(&self.input_packet_cache, id))
//             .collect::<Result<Vec<_>>>()?;

//         // Add the new incoming packet as a factor
//         let incoming_packet = vec![new_packet.clone()];
//         factors.push(&incoming_packet);

//         let result = factors
//             .into_iter()
//             .multi_cartesian_product()
//             .map(|packets_to_combined| {
//                 packets_to_combined
//                     .into_iter()
//                     .fold(HashMap::new(), |mut acc, packet| {
//                         acc.extend(packet.clone());
//                         acc
//                     })
//             })
//             .collect::<Vec<_>>();

//         Ok(result)
//     }
// }

// impl NodeProcessor for JoinerNodeProcessor {
//     fn process_packet(
//         &mut self,
//         sender_node_id: String,
//         current_node_id: String,
//         packet: HashMap<String, PathSet>,
//         success_ch_tx: Sender<Message>,
//         failure_ch_tx: Sender<Message>,
//     ) -> Result<()> {
//         let process_result = {
//             // Compute the new packet combination based on the sender node id and the packet
//             let new_packets_to_send =
//                 self.compute_new_packet_combination(&sender_node_id, &packet)?;

//             // Record the packet into the cache
//             self.input_packet_cache
//                 .get_mut(&sender_node_id)
//                 .context(selector::KeyMissing {
//                     key: sender_node_id.clone(),
//                 })?
//                 .push(packet);

//             Ok::<Vec<HashMap<String, PathSet>>, OrcaError>(new_packets_to_send)
//         };

//         match process_result {
//             Ok(output_packets) => {
//                 // Send the output packets to the success channel
//                 for output_packet in output_packets {
//                     success_ch_tx
//                         .send(Message::NodeOutput(current_node_id.clone(), output_packet))?;
//                 }
//             }
//             Err(err) => {
//                 // Send the error to the failure channel
//                 failure_ch_tx.send(Message::NodeOutput(
//                     sender_node_id.clone(),
//                     HashMap::new(), // Empty packet on failure
//                 ))?;
//                 return Err(err);
//             }
//         }
//         // Add the new packet into the cache

//         Ok(())
//     }
// }
