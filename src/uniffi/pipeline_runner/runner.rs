use super::PipelineRun;
use crate::{
    core::{crypto::hash_buffer, model::serialize_hashmap, util::get},
    uniffi::{
        error::{OrcaError, Result, selector},
        model::{PathSet, Pod, PodJob, URI},
        pipeline::{Kernel, Mapper, Node, PipelineJob, PipelineResult},
    },
};
use futures_util::future::try_join_all;
use itertools::Itertools as _;
use serde_yaml::Serializer;
use snafu::OptionExt as _;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};
use tokio::{
    sync::{RwLock, mpsc},
    task::{JoinSet, spawn_blocking},
};

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
    node_tx: HashMap<String, mpsc::Sender<Message>>,
    outputs: Arc<RwLock<HashMap<String, Vec<HashMap<String, PathSet>>>>>, // String is the node key, while hash
}

/**
 * Runner for pipelines
 *
 * General Algorithm:
 * 1. All nodes receive inputs via a MPSC channel, where parents nodes will send their output packets
 * 2. There are two "functional nodes processor" in the pipeline,
 *    which is the `input_node` and `output_node`
 * 3. Each node will process the inputs its receives and will only send it children input channels
 *    if they are successfully processed. Failures are just printed for now (Will be replaced by logging)
 */
#[derive(Default)]
pub struct DockerPipelineRunner {
    pipeline_runs: HashMap<PipelineRun, PipelineRunInfo>, // For each pipeline run, we have a join set to track the tasks and wait on them
}

impl DockerPipelineRunner {
    /// Create a new Docker pipeline runner
    pub fn new() -> Self {
        Self::default()
    }

    /**
    Start the `pipeline_job` returning `pipeline_run`

    Algorithm:
    1. Create a new `PipelineRun` from the `pipeline_job`
    2. Insert the `PipelineRun` into the `pipeline_runs` map
    3. Create an output channel to capture the outputs of the nodes
       (This will be given to the output capture task)
    4. Create a task that captures the outputs form nodes and stores them in the `outputs` map
       This is done via listening the channel and acting like a final node in the pipeline
    5. Get the root nodes of the pipeline and call `create_task_for_node` for each root node
       This will recursively BFS through the pipeline and create tasks for each node
       (More detail in that function)
    6. Using the `root_nodes` txs, we will send all inputs to that channel.
       This will start the pipeline execution
    7. Upon sending all the inputs, we will send node complete message
       signifying that the `input_node` is done
    8. Return the `PipelineRun` which can be used to get the results later

    # Errors
    Will error out if the pipeline job fails to start
    */
    pub async fn start(
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
                node_tx: HashMap::new(),
                node_task_join_set: JoinSet::new(),
                outputs: Arc::new(RwLock::new(HashMap::new())),
            },
        );

        // Get reference to the pipeline
        let pipeline = &pipeline_run_arc.pipeline_job.pipeline;

        // Create the output channel to capture the outputs of the outputs nodes (Currently only leaf nodes)
        let (output_tx, mut output_rx) = mpsc::channel::<Message>(128); // Channel to capture outputs from nodes

        // Insert the output channel into the pipeline run info
        self.pipeline_runs
            .get_mut(&pipeline_run_arc)
            .context(selector::KeyMissing {
                key: pipeline_run_arc.to_string(),
            })?
            .node_tx
            .insert("output".to_owned(), output_tx.clone());

        // Get the output_nodes (leaf nodes for now) so the output task can keep track when parents are done
        let output_nodes_ids = pipeline
            .get_leaf_nodes()
            .map(|node| node.id.clone())
            .collect::<HashSet<_>>();
        let outputs = Arc::clone(&get(&self.pipeline_runs, &pipeline_run_arc)?.outputs);

        // Create the task that captures the output from the nodes and stores them in the outputs map
        self.pipeline_runs
            .get_mut(&pipeline_run_arc)
            .context(selector::KeyMissing {
                key: pipeline_run_arc.to_string(),
            })?
            .node_task_join_set
            .spawn(async move {
                let mut complete_parent_nodes = HashSet::new();
                while let Some(message) = output_rx.recv().await {
                    match message {
                        Message::NodeOutput(sender_node_id, hash_map) => {
                            // Store the output in the outputs map
                            outputs
                                .write()
                                .await
                                .entry(sender_node_id)
                                .or_default()
                                .push(hash_map);
                        }
                        Message::NodeProcessingComplete(sender_node_id) => {
                            // Add the sender node id to the complete parent nodes
                            complete_parent_nodes.insert(sender_node_id.clone());

                            // Check if all parent nodes are complete
                            if complete_parent_nodes.is_superset(&output_nodes_ids) {
                                // All parents are complete, we can exit this task
                                return Ok(());
                            }
                        }
                        Message::Stop => {
                            // No clear action needed, just exit the task
                            return Ok(());
                        }
                    }
                }
                Ok(())
            });

        // Get all the root nodes and call the create_task_for_node function for each root node
        // This will recursively create all the tasks and channels for the pipeline
        let root_nodes_tx = pipeline
            .get_root_nodes()
            .map(|node| {
                self.create_task_for_node(node, &pipeline_run_arc, &output_tx, namespace_lookup)
            })
            .collect::<Result<Vec<_>>>()?;

        // All pipeline tasks have been created, now we need to feed the inputs to the pipeline
        for tx in &root_nodes_tx {
            for input_packet in &pipeline_run.pipeline_job.input_packets {
                tx.send(Message::NodeOutput(
                    "input".to_owned(),
                    input_packet.clone(),
                ))
                .await?;
            }
        }

        // Send a message that all job inputs have been sent
        for tx in &root_nodes_tx {
            tx.send(Message::NodeProcessingComplete("input".to_owned()))
                .await?;
        }

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
            output_packets: pipeline_run_info.outputs.read().await.clone(),
        })
    }

    /// Stop the pipeline run and all its tasks
    /// # Errors
    /// Will error out if the pipeline run is not found or if any of the tasks fail to stop correctly
    pub async fn stop(&mut self, pipeline_run: &PipelineRun) -> Result<()> {
        // Get the pipeline run info
        let pipeline_run_info =
            self.pipeline_runs
                .get_mut(pipeline_run)
                .context(selector::KeyMissing {
                    key: pipeline_run.to_string(),
                })?;

        // Send a stop message to all the node txs
        for tx in pipeline_run_info.node_tx.values() {
            tx.send(Message::Stop).await?;
        }

        // Wait for all tasks to complete
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

        // Remove the pipeline run from the list of pipeline runs
        self.pipeline_runs.remove(pipeline_run);

        Ok(())
    }

    /// Helper function to create a task for each node, while recursively BFS through the pipeline
    /// Summary:
    /// 1. Check if their is already a channel created for the node, if not create one and insert it
    /// 2. Call this function for each of the child nodes to get their `Sender_tx`
    /// 3. If the node is a leaf node, attach the `output_tx` to the tx (Will be replaced by `output_nodes`)
    /// 4. Start the task manager for the node, which will act as the node's processor
    fn create_task_for_node(
        &mut self,
        node: &Node,
        pipeline_run: &Arc<PipelineRun>,
        output_tx: &mpsc::Sender<Message>,
        namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<mpsc::Sender<Message>> {
        println!("Creating task for node: {}", node.id);
        // Create a channel for the node

        // Use closer to limit the scope of the borrow
        let (tx, rx) = {
            let pipeline_info =
                self.pipeline_runs
                    .get_mut(pipeline_run)
                    .context(selector::KeyMissing {
                        key: pipeline_run.to_string(),
                    })?;
            // Check if the node is already inside the node_tx
            if pipeline_info.node_tx.contains_key(&node.id) {
                // Node already exists, thus we can return the existing tx
                return Ok(get(&pipeline_info.node_tx, &node.id)?.clone());
            }

            // This channel will be used to send messages to the node processor
            let (tx, rx) = mpsc::channel::<Message>(128);

            // Record the tx into the pipeline_info tx_hashmap
            pipeline_info.node_tx.insert(node.id.clone(), tx.clone());
            (tx, rx)
        };

        // Call this function for each of the child nodes to get their Sender_tx
        let mut children_node_tx = pipeline_run
            .pipeline_job
            .pipeline
            .get_children_for_node(node)
            .map(|child_node| {
                self.create_task_for_node(child_node, pipeline_run, output_tx, namespace_lookup)
            })
            .collect::<Result<Vec<_>>>()?;

        // Check if children_node_tx is empty, if so, this is a leaf node thus we need to attach the output_tx
        if children_node_tx.is_empty() {
            // This is a leaf node, thus we need to attach the output_tx to the tx
            // This will allow the node to send its output to the output channel
            children_node_tx.push(output_tx.clone());
        }

        // Start the task_manager
        self.pipeline_runs
            .get_mut(pipeline_run)
            .context(selector::KeyMissing {
                key: pipeline_run.to_string(),
            })?
            .node_task_join_set
            .spawn(Self::start_node_manager(
                node.clone(),
                Arc::clone(pipeline_run),
                rx,
                children_node_tx,
                namespace_lookup.clone(),
            ));

        Ok(tx)
    }

    /// Act as the processor of the node by:
    /// 1. Creating a metadata struct for the node to be passed to the appropriate processor
    /// 2. Get the kernel for the node and build the correct processor for this node
    /// 3. Start the processor and wait till it completes
    /// 4. Send a message that the node processing is complete
    ///
    /// # Errors
    /// Will error out if the kernel for the node is not found or if the
    async fn start_node_manager(
        node: Node,
        pipeline_run: Arc<PipelineRun>,
        node_rx: mpsc::Receiver<Message>,
        success_chs_tx: Vec<mpsc::Sender<Message>>,
        namespace_lookup: HashMap<String, PathBuf>,
    ) -> Result<()> {
        // Create a metadata struct for this node
        let node_metadata = NodeMetaData {
            node_id: node.id.clone(),
            node_rx,
            child_nodes_txs: success_chs_tx.clone(),
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
                processor.start().await;
            }
            Kernel::Mapper(mapper) => {
                let mut processor = MapperProcessor::new(Arc::clone(mapper), node_metadata);
                processor.start().await;
            }
            Kernel::Joiner => {
                let parent_nodes_id = pipeline_run
                    .pipeline_job
                    .pipeline
                    .get_parents_for_node(&node)
                    .map(|parent_node| parent_node.id.clone())
                    .collect::<Vec<_>>();
                let mut processor = JoinerProcessor::new(parent_nodes_id, node_metadata);
                processor.start().await;
            }
        }

        // Since all inputs are sent, we can send a message that the "input node" processing is complete
        for success_ch_tx in &success_chs_tx {
            match success_ch_tx
                .send(Message::NodeProcessingComplete(node.id.clone()))
                .await
            {
                Ok(()) => {}
                Err(err) => {
                    match err {
                        mpsc::error::SendError(Message::NodeProcessingComplete(_)) => {
                            // The channel is closed, we can ignore this error, this happens when stop it called
                            eprintln!("Failed to send processing complete message, channel closed");
                        }
                        _ => {
                            eprintln!("Failed to send processing complete message: {err}");
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

/// Metadata for the node processor
/// Contains fields that is normally needed to process incoming packets
struct NodeMetaData {
    node_id: String,
    node_rx: mpsc::Receiver<Message>, // Channel to listen to messages from parent nodes
    child_nodes_txs: Vec<mpsc::Sender<Message>>, // Channel to send successful outputs to the next node
    namespace: String,
    namespace_lookup: HashMap<String, PathBuf>, // Copy of the look up table
}

/// Unify the interface for node processors and provide a common way to handle processing of incoming messages
/// This trait defines the methods that all node processors should implement
///
/// Main purpose was to reduce the amount of code duplication between different node processors
/// As a result, each processor only needs to worry about writing their own function to process the msg.
pub(crate) trait NodeProcessor {
    fn get_node_rx(&mut self) -> &mut mpsc::Receiver<Message>;

    async fn start(&mut self) {
        // Start to listen to the channels
        // Listen to the MPSC channel and handle messages
        while let Some(msg) = self.get_node_rx().recv().await {
            if self.process_msg(msg).await {
                // If the message indicates that processing is complete, we can exit the loop
                // Wait for all processing tasks to complete before returning
                self.wait_for_node_task_completion().await;
                break;
            }
        }
    }

    async fn process_msg(&mut self, msg: Message) -> bool;

    async fn wait_for_node_task_completion(&mut self);
}

/// Processor for Pods
/// Currently missing implementation to call agents for actual pod processing
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

    /// Actual logic of processing a packet using the pod
    /// At the moment it does a simulation of pod execution
    async fn process_packet(
        node_id: String,
        pod: Arc<Pod>,
        namespace: String,
        namespace_lookup: HashMap<String, PathBuf>,
        packet: HashMap<String, PathSet>,
        success_chs_tx: Vec<mpsc::Sender<Message>>,
    ) -> Result<()> {
        // Process the packet using the pod
        // Create the pod_job

        // We need a unique hash for this given input packet process by the node
        // therefore we need to generate a hash that has the pod_id + input_packet
        let node_id_bytes = node_id.as_bytes().to_vec();
        let packet_copy = packet.clone();
        let input_packet_hash = spawn_blocking(move || {
            let mut buf = node_id_bytes;
            let mut serializer = Serializer::new(&mut buf);
            serialize_hashmap(&packet_copy, &mut serializer)?;
            Ok::<_, OrcaError>(hash_buffer(buf))
        })
        .await??;
        let output_dir = URI {
            namespace: namespace.clone(),
            path: PathBuf::from(format!("pod_runs/{}/{}", pod.hash, input_packet_hash)),
        };

        let cpu_limit = pod.recommended_cpus;
        let memory_limit = pod.recommended_memory;

        // Create the pod job
        let pod_job = PodJob::new(
            None,
            Arc::clone(&pod),
            packet.clone(),
            output_dir,
            cpu_limit,
            memory_limit,
            None,
            &namespace_lookup,
        )?;

        // Simulate pod execution by just printing out pod_job_hash and pod hash
        // This will be replaced by sending the pod_job to the orchestrator via the agent
        println!(
            "Simulating Executing pod job: {} with pod hash: {}",
            pod_job.hash, pod_job.pod.hash
        );

        #[expect(
            clippy::unwrap_used,
            reason = "Hard code for now, will be replaced by agent"
        )]
        // Build the output_packet
        let output_packet = pod
            .output_spec
            .keys()
            .map(|output_key| (output_key.clone(), packet.values().next().cloned().unwrap()))
            .collect::<HashMap<_, _>>();

        // For now we will just send the input_packet to the success channel
        try_join_all(success_chs_tx.iter().map(|success_ch_tx| {
            success_ch_tx.send(Message::NodeOutput(node_id.clone(), output_packet.clone()))
        }))
        .await?;

        Ok(())
    }
}

impl NodeProcessor for PodProcessor {
    fn get_node_rx(&mut self) -> &mut mpsc::Receiver<Message> {
        &mut self.node_metadata.node_rx
    }

    async fn process_msg(&mut self, msg: Message) -> bool {
        match msg {
            Message::NodeOutput(_, packet) => {
                let pod_ref = Arc::clone(&self.pod);
                let node_id = self.node_metadata.node_id.clone();
                let namespace = self.node_metadata.namespace.clone();
                let namespace_lookup = self.node_metadata.namespace_lookup.clone();
                let child_nodes_txs = self.node_metadata.child_nodes_txs.clone();
                // Forward it into a processing task
                self.processing_tasks.spawn(async move {
                    // Process the packet using the pod
                    // This will execute the pod and send the output to the next node
                    if let Err(err) = Self::process_packet(
                        node_id,
                        pod_ref,
                        namespace,
                        namespace_lookup,
                        packet,
                        child_nodes_txs,
                    )
                    .await
                    {
                        // Send the error to the failure channel
                        // For now just print it out
                        eprintln!("Failed to process packet with error: {err}");
                    }
                    Ok(())
                });
            }
            Message::Stop => {
                // Stop message received, we will stop processing
                self.processing_tasks.abort_all();
                return true;
            }
            Message::NodeProcessingComplete(_) => {
                // Since pod only have one parent, we can expect that there will be no more incoming packet
                // thus, we need to wait for everything to finish processing and send completion message
                // Return true to notify caller that processing is complete
                self.wait_for_node_task_completion().await;
                return true;
            }
        }
        false
    }

    async fn wait_for_node_task_completion(&mut self) {
        while self.processing_tasks.join_next().await.is_some() {
            // Wait for all processing tasks to complete
        }
    }
}

/// Processor for Mapper nodes
/// This processor renames the `input_keys` from the input packet to the `output_keys` defined by the map
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

    async fn process_packet(&self, packet: &HashMap<String, PathSet>) -> Result<()> {
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
        try_join_all(self.node_metadata.child_nodes_txs.iter().map(|ch| {
            ch.send(Message::NodeOutput(
                self.node_metadata.node_id.clone(),
                output_map.clone(),
            ))
        }))
        .await?;
        Ok(())
    }
}

impl NodeProcessor for MapperProcessor {
    fn get_node_rx(&mut self) -> &mut mpsc::Receiver<Message> {
        &mut self.node_metadata.node_rx
    }

    async fn wait_for_node_task_completion(&mut self) {
        // Mapper doesn't spawn additional tasks, so this is a no-op
    }

    async fn process_msg(&mut self, msg: Message) -> bool {
        match msg {
            Message::NodeOutput(_, packet) => {
                match self.process_packet(&packet).await {
                    Ok(()) => {}
                    Err(err) => {
                        // Send the error to the failure channel
                        // For now just print it out
                        eprintln!("Failed to process packet with error: {err}");
                    }
                }
            }
            Message::NodeProcessingComplete(_) | Message::Stop => return true,
        }

        false
    }
}

/// Processor for Joiner nodes
/// This processor combines packets from multiple parent nodes into a single output packet
/// It uses a cartesian product to combine packets from different parents
struct JoinerProcessor {
    /// Cache for all packets received by the node
    input_packet_cache: HashMap<String, Vec<HashMap<String, PathSet>>>,
    completed_parents: Vec<String>,
    node_metadata: NodeMetaData,
    initial_computation_completed: bool,
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
            initial_computation_completed: false,
        }
    }

    fn compute_new_packet_combination(
        &mut self,
        sender_node_id: &str,
        new_packet: &HashMap<String, PathSet>,
    ) -> Result<Vec<HashMap<String, PathSet>>> {
        // Combine the new packet with the existing packets in the cache
        // Get all the cached packets from other parents
        let other_parent_ids = self
            .input_packet_cache
            .keys()
            .filter(|key| *key != sender_node_id);

        // Create a vector to hold the incoming packet
        // This will be used to compute the cartesian product and will be modified if the initial computation is not completed
        let mut incoming_packet = vec![new_packet.clone()];

        // Determine if the initial computation has been computed
        if !self.initial_computation_completed {
            // Check if we at least have one cached packet for each of the other parents
            for parent_id in other_parent_ids.clone() {
                if get(&self.input_packet_cache, parent_id)?.is_empty() {
                    // We are still missing other parents, so we can't compute the new packet combination yet
                    return Ok(Vec::new());
                }
            }

            // We have at least one packet for each of the other parents, thus we can compute the cartesian product
            // For the initial computation, we will add all of the add all previous packets for this sender
            get(&self.input_packet_cache, &sender_node_id.to_owned())?
                .iter()
                .for_each(|packet| incoming_packet.push(packet.clone()));

            self.initial_computation_completed = true;
        }

        let mut factors = other_parent_ids
            .map(|id| get(&self.input_packet_cache, id))
            .collect::<Result<Vec<_>>>()?;

        // Add the new incoming packet as a factor

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

    async fn process_packet(
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
                    try_join_all(self.node_metadata.child_nodes_txs.iter().map(|ch| {
                        ch.send(Message::NodeOutput(
                            self.node_metadata.node_id.clone(),
                            output_packet.clone(),
                        ))
                    }))
                    .await?;
                }
            }
            Err(err) => {
                // Send the error to the failure channel
                eprintln!(
                    "Failed to process packet from {sender_node_id} for joiner node with error: {err}"
                );
            }
        }
        // Add the new packet into the cache

        Ok(())
    }
}

impl NodeProcessor for JoinerProcessor {
    fn get_node_rx(&mut self) -> &mut mpsc::Receiver<Message> {
        &mut self.node_metadata.node_rx
    }

    async fn wait_for_node_task_completion(&mut self) {
        // Joiner doesn't spawn additional tasks, so this is a no-op
    }

    async fn process_msg(&mut self, msg: Message) -> bool {
        match msg {
            Message::NodeOutput(sender_node_id, packet) => {
                // Process the packet and send the output to the success channel
                match self.process_packet(&sender_node_id, packet).await {
                    Ok(()) => {}
                    Err(err) => {
                        // Send the error to the failure channel
                        eprintln!("Failed to process packet with error: {err}");
                    }
                }
            }
            Message::NodeProcessingComplete(sender_node_id) => {
                // Record that this parent node has completed processing
                self.completed_parents.push(sender_node_id);

                // Check if all parents have completed processing
                if self.completed_parents.len() == self.input_packet_cache.len() {
                    // All parents have completed processing, we can send the output
                    // Wait for all packets to be processed and send the output
                    return true;
                }
            }
            Message::Stop => {
                // We don't have anything to clean up, so we can just return
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[expect(clippy::panic_in_result_fn, reason = "Unit test")]
async fn joiner() -> Result<()> {
    // Create a fake mpsc channel for the node
    let (_, node_rx) = mpsc::channel::<Message>(128);

    // Create a child mpsc
    let (child_tx, mut child_rx) = mpsc::channel::<Message>(128);

    let node_metadata = NodeMetaData {
        node_id: "joiner_node".to_owned(),
        node_rx,
        child_nodes_txs: vec![child_tx],
        namespace: "test".to_owned(),
        namespace_lookup: HashMap::new(),
    };

    let mut joiner_process = JoinerProcessor::new(
        vec!["0".to_owned(), "1".to_owned(), "2".to_owned()],
        node_metadata,
    );

    // Make each parent has 1 packet
    for idx in 0..2 {
        joiner_process
            .process_packet(
                &format!("{idx}"),
                make_test_packet("data_1.txt".to_owned().into()),
            )
            .await?;
    }

    // Confirm that there should be no output yet

    // Now we send the missing parent package
    // This will yield one unique combination
    joiner_process
        .process_packet("2", make_test_packet("data_1.txt".to_owned().into()))
        .await?;

    // Confirm that the output is sent to the child channel
    assert!(
        child_rx.len() == 1,
        "Should have only one message in the channel",
    );
    assert!(
        child_rx.recv().await.is_some(),
        "Should have received a message"
    );

    // Insert another one
    joiner_process
        .process_packet("2", make_test_packet("data_2.txt".to_owned().into()))
        .await?;

    // The joiner node should send another one
    assert!(
        child_rx.len() == 1,
        "Should have only one message in the channel",
    );
    assert!(
        child_rx.recv().await.is_some(),
        "Should have received a message"
    );

    // Now insert to packet for parent 0, which should yield 2 packets in total
    // This is because of the cartesian product
    joiner_process
        .process_packet("0", make_test_packet("data_2.txt".to_owned().into()))
        .await?;

    assert!(
        child_rx.len() == 2,
        "Should have only two messages in the channel",
    );
    assert!(
        child_rx.recv().await.is_some(),
        "Should have received a message"
    );

    Ok(())
}

#[cfg(test)]
fn make_test_packet(path: PathBuf) -> HashMap<String, PathSet> {
    use crate::uniffi::model::{Blob, BlobKind};

    let path_set = PathSet::Unary(Blob {
        kind: BlobKind::File,
        location: URI {
            namespace: "test".to_owned(),
            path,
        },
        checksum: String::new(),
    });

    HashMap::from([("key".to_owned(), path_set)])
}
