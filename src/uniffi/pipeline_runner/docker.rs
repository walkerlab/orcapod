use super::PipelineRun;
use crate::{
    core::{crypto::hash_buffer, model::serialize_hashmap, util::get},
    uniffi::{
        error::{Kind, OrcaError, Result, selector},
        model::{PathSet, Pod, PodJob, URI},
        pipeline::{Kernel, Mapper, Node, PipelineJob, PipelineResult},
    },
};
use futures_util::stream::FuturesUnordered;
use itertools::Itertools as _;
use serde_yaml::Serializer;
use snafu::OptionExt as _;
use std::{backtrace::Backtrace, collections::HashMap, path::PathBuf, sync::Arc};
use tokio::{
    sync::{
        RwLock,
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
    Stop, // Message to halt all operations
}

#[expect(
    clippy::type_complexity,
    reason = "too complex, but necessary for async handling"
)]
struct PipelineRunInfo {
    node_task_join_set: JoinSet<Result<()>>, // Join set to track the tasks for this pipeline run
    job_manager_ch_tx: Sender<Message>,
    node_tx: HashMap<String, Sender<Message>>,
    outputs: Arc<RwLock<HashMap<String, Vec<HashMap<String, PathSet>>>>>, // String is the node key, while hash
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
                outputs: Arc::new(RwLock::new(HashMap::new())),
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

            // Since we don't have output nodes implemented, and currently it is set as leaf nodes,
            // we can do the output handling logic here too

            Ok::<(), OrcaError>(())
        })?;

        // All pipeline tasks have been created, now we need to feed the inputs to the pipeline
        pipeline_run
            .pipeline_job
            .input_packets
            .iter()
            .try_for_each(|input_map| {
                source_tx.send(Message::NodeOutput("input".to_owned(), input_map.clone()))?;
                Ok::<(), OrcaError>(())
            })?;

        // Send a message that all job inputs have been sent
        source_tx.send(Message::NodeProcessingComplete("input".to_owned()))?;

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

    fn create_task_to_capture_output_of_node(
        &mut self,
        node: &Node,
        pipeline_run: &Arc<PipelineRun>,
    ) -> Result<()> {
        let pipeline_run_info =
            self.pipeline_runs
                .get_mut(pipeline_run)
                .context(selector::KeyMissing {
                    key: pipeline_run.to_string(),
                })?;
        // Get the output ch rx for the node
        let node_rx = get(&pipeline_run_info.node_tx, &node.id)?.subscribe();
        // Create a new ref copy of pipeline_run_output
        let outputs_ref = Arc::clone(&pipeline_run_info.outputs);
        // Create a task to listen to it and record the outputs
        pipeline_run_info
            .node_task_join_set
            .spawn(Self::capture_node_output(node_rx, outputs_ref));

        Ok(())
    }

    #[expect(
        clippy::type_complexity,
        reason = "too complex, but necessary for async handling"
    )]
    async fn capture_node_output(
        mut node_rx: Receiver<Message>,
        outputs_ref: Arc<RwLock<HashMap<String, Vec<HashMap<String, PathSet>>>>>,
    ) -> Result<()> {
        loop {
            let message = match node_rx.recv().await {
                Ok(message) => message,
                Err(err) => {
                    match err {
                        RecvError::Closed => {
                            // No more message will be received, thus we can exit the loop
                            // Only case where this will occur is when the channel is closed due to abort
                            break;
                        }
                        RecvError::Lagged(_) => {
                            print!("Warning: Channel lagged, skipping message");
                        }
                    }
                    continue;
                }
            };
            match message {
                Message::NodeOutput(node_id, hash_map) => {
                    // Record the output

                    outputs_ref
                        .write()
                        .await
                        .entry(node_id)
                        .or_default()
                        .push(hash_map);
                }
                Message::NodeProcessingComplete(_) | Message::Stop => {
                    // Node processing is complete, we can stop listening to this channel
                    break;
                }
            }
        }

        Ok(())
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
        let (processing_complete_ch_tx, processing_complete_ch_rx) = oneshot::channel::<()>();

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

        // Create a metadata struct for this node
        let node_metadata = NodeMetaData {
            node_id: node.id.clone(),
            ch_to_listen_to: chs_to_listen_to,
            success_ch_tx: success_ch_tx.clone(),
            namespace: pipeline_run.pipeline_job.output_dir.namespace.clone(),
            namespace_lookup: namespace_lookup.clone(),
        };

        // Get the kernel for this node and build the correct processor
        match get(
            &pipeline_run.pipeline_job.pipeline.kernel_lut,
            &node.kernel_hash,
        )? {
            Kernel::Pod(pod) => {
                let mut processor = PodProcessor::new(Arc::clone(pod), node_metadata);
                processor.start(processing_complete_ch_tx).await?;
                processing_complete_ch_rx.await?;
            }
            Kernel::Mapper(mapper) => {
                let mut processor = MapperProcessor::new(Arc::clone(mapper), node_metadata);
                processor.start(processing_complete_ch_tx).await?;
                processing_complete_ch_rx.await?;
            }
            Kernel::Joiner => {
                let parent_nodes_id = pipeline_run
                    .pipeline_job
                    .pipeline
                    .get_parents_for_node(&node)
                    .map(|parent_node| parent_node.id.clone())
                    .collect::<Vec<_>>();
                let mut processor = JoinerProcessor::new(parent_nodes_id, node_metadata);
                processor.start(processing_complete_ch_tx).await?;
                processing_complete_ch_rx.await?;
            }
        }

        Ok(())
    }
}

struct NodeMetaData {
    node_id: String,
    ch_to_listen_to: FuturesUnordered<JoinHandle<Result<Message, RecvError>>>,
    success_ch_tx: Sender<Message>, // Channel to send successful outputs to the next node
    namespace: String,
    namespace_lookup: HashMap<String, PathBuf>, // Copy of the look up table
}

trait NodeProcessor {
    fn get_ch_to_listen_to(
        &mut self,
    ) -> &mut FuturesUnordered<JoinHandle<Result<Message, RecvError>>>;

    async fn wait_for_node_task_completion(&mut self);

    async fn start(&mut self, process_complete_ch_tx: oneshot::Sender<()>) -> Result<()> {
        // Start to listen to the channels
        // Listen to the MPSC channel and handle messages
        while let Some(result) = self.get_ch_to_listen_to().next().await {
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

            // Process the message
            if self.process_msg(msg).await? {
                // If the message indicates that processing is complete, we can exit the loop
                // Wait for all processing tasks to complete before sending the completion message

                self.wait_for_node_task_completion().await;

                // Send the node processing complete message
                process_complete_ch_tx.send(()).map_err(|()| OrcaError {
                    kind: Kind::ReceiverDroppedBeforeSender {
                        backtrace: Some(Backtrace::capture()),
                    },
                })?;
                break;
            }
        }

        Ok(())
    }

    async fn process_msg(&mut self, msg: Message) -> Result<bool>;
}

struct PodProcessor {
    pod: Arc<Pod>,
    node_metadata: NodeMetaData,
    processing_tasks: JoinSet<Result<(), OrcaError>>,
}

impl PodProcessor {
    fn new(pod: Arc<Pod>, node_metadata: NodeMetaData) -> Self {
        Self {
            pod,
            node_metadata,
            processing_tasks: JoinSet::new(),
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

impl NodeProcessor for PodProcessor {
    async fn process_msg(&mut self, msg: Message) -> Result<bool> {
        match msg {
            Message::NodeOutput(sender_node_id, packet) => {
                let pod_ref = Arc::clone(&self.pod);
                let node_id = self.node_metadata.node_id.clone();
                let namespace = self.node_metadata.namespace.clone();
                let namespace_lookup = self.node_metadata.namespace_lookup.clone();
                let success_ch_tx = self.node_metadata.success_ch_tx.clone();
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
                // Stop message received, we will stop processing
                self.processing_tasks.abort_all();
                return Ok(true);
            }
            Message::NodeProcessingComplete(_) => {
                // Since pod only have one parent, we can expect that there will be no more incoming packet
                // thus, we need to wait for everything to finish processing and send completion message
                // Return true to notify caller that processing is complete
                self.wait_for_node_task_completion().await;
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn get_ch_to_listen_to(
        &mut self,
    ) -> &mut FuturesUnordered<JoinHandle<Result<Message, RecvError>>> {
        &mut self.node_metadata.ch_to_listen_to
    }

    async fn wait_for_node_task_completion(&mut self) {
        while self.processing_tasks.join_next().await.is_some() {
            // Wait for all processing tasks to complete
        }
    }
}

struct MapperProcessor {
    mapper: Arc<Mapper>,
    node_metadata: NodeMetaData,
}

impl MapperProcessor {
    const fn new(mapper: Arc<Mapper>, node_metadata: NodeMetaData) -> Self {
        Self {
            mapper,
            node_metadata,
        }
    }

    fn process_packet(&self, packet: &HashMap<String, PathSet>) -> Result<()> {
        // Apply the mapping to the input packet
        let output_map = self
            .mapper
            .mapping
            .iter()
            .map(|(input_key, output_key)| {
                let input = get(packet, input_key)?.clone();
                Ok((output_key.to_owned(), input))
            })
            .collect::<Result<HashMap<_, _>>>()?;

        // Send the output via the channel
        self.node_metadata.success_ch_tx.send(Message::NodeOutput(
            self.node_metadata.node_id.clone(),
            output_map,
        ))?;
        Ok(())
    }
}

impl NodeProcessor for MapperProcessor {
    fn get_ch_to_listen_to(
        &mut self,
    ) -> &mut FuturesUnordered<JoinHandle<Result<Message, RecvError>>> {
        &mut self.node_metadata.ch_to_listen_to
    }

    async fn wait_for_node_task_completion(&mut self) {
        // Mapper doesn't spawn additional tasks, so this is a no-op
    }

    async fn process_msg(&mut self, msg: Message) -> Result<bool> {
        match msg {
            Message::NodeOutput(_, hash_map) => {
                let output_map = self
                    .mapper
                    .mapping
                    .iter()
                    .map(|(input_key, output_key)| {
                        let input = get(&hash_map, input_key)?.clone();
                        Ok((output_key.to_owned(), input))
                    })
                    .collect::<Result<HashMap<_, _>>>()?;

                // For now we will just send the input_packet to the success channel
                self.node_metadata.success_ch_tx.send(Message::NodeOutput(
                    self.node_metadata.node_id.clone(),
                    output_map,
                ))?;
            }
            Message::NodeProcessingComplete(_) => return Ok(true),
            Message::Stop => todo!(),
        }

        Ok(false)
    }
}

struct JoinerProcessor {
    /// Cache for all packets received by the node
    input_packet_cache: HashMap<String, Vec<HashMap<String, PathSet>>>,
    completed_parents: Vec<String>,
    node_metadata: NodeMetaData,
}

impl JoinerProcessor {
    fn new(parents_node_id: Vec<String>, node_metadata: NodeMetaData) -> Self {
        let input_packet_cache = parents_node_id
            .into_iter()
            .map(|id| (id, Vec::new()))
            .collect();
        Self {
            input_packet_cache,
            node_metadata,
            completed_parents: Vec::new(),
        }
    }

    fn compute_new_packet_combination(
        &self,
        sender_node_id: &str,
        new_packet: &HashMap<String, PathSet>,
    ) -> Result<Vec<HashMap<String, PathSet>>> {
        // Combine the new packet with the existing packets in the cache
        // Get all the cached packets from other parents
        let other_parent_ids = self
            .input_packet_cache
            .keys()
            .filter(|key| *key != sender_node_id);
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

    fn process_packet(
        &mut self,
        sender_node_id: &str,
        packet: HashMap<String, PathSet>,
    ) -> Result<()> {
        let process_result = {
            // Compute the new packet combination based on the sender node id and the packet
            let new_packets_to_send =
                self.compute_new_packet_combination(sender_node_id, &packet)?;

            // Record the packet into the cache
            self.input_packet_cache
                .get_mut(sender_node_id)
                .context(selector::KeyMissing {
                    key: sender_node_id.to_owned(),
                })?
                .push(packet);

            Ok::<Vec<HashMap<String, PathSet>>, OrcaError>(new_packets_to_send)
        };

        match process_result {
            Ok(output_packets) => {
                // Send the output packets to the success channel
                for output_packet in output_packets {
                    self.node_metadata.success_ch_tx.send(Message::NodeOutput(
                        self.node_metadata.node_id.clone(),
                        output_packet,
                    ))?;
                }
            }
            Err(err) => {
                // Send the error to the failure channel
                todo!();
            }
        }
        // Add the new packet into the cache

        Ok(())
    }
}

impl NodeProcessor for JoinerProcessor {
    fn get_ch_to_listen_to(
        &mut self,
    ) -> &mut FuturesUnordered<JoinHandle<Result<Message, RecvError>>> {
        &mut self.node_metadata.ch_to_listen_to
    }

    async fn wait_for_node_task_completion(&mut self) {
        // Joiner doesn't spawn additional tasks, so this is a no-op
    }

    async fn process_msg(&mut self, msg: Message) -> Result<bool> {
        match msg {
            Message::NodeOutput(sender_node_id, packet) => {
                // Process the packet and send the output to the success channel
                self.process_packet(&sender_node_id, packet)?;
            }
            Message::NodeProcessingComplete(sender_node_id) => {
                // Record that this parent node has completed processing
                self.completed_parents.push(sender_node_id);

                // Check if all parents have completed processing
                if self.completed_parents.len() == self.input_packet_cache.len() {
                    // All parents have completed processing, we can send the output
                    // Wait for all packets to be processed and send the output
                    return Ok(true);
                }
            }
            Message::Stop => {
                // We don't have anything to clean up, so we can just return
                return Ok(true);
            }
        }

        Ok(false)
    }
}
