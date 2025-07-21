use crate::{
    core::{crypto::hash_buffer, model::serialize_hashmap, util::get},
    uniffi::{
        error::{OrcaError, Result, selector},
        model::{Blob, BlobKind, PathSet, Pod, PodJob, URI},
        pipeline::{Kernel, Mapper, Node, Pipeline, PipelineJob, PipelineResult},
    },
};
use async_trait::async_trait;
use itertools::Itertools as _;
use serde::{Deserialize, Serialize};
use serde_yaml::Serializer;
use snafu::{OptionExt as _, ResultExt as _};
use std::{
    collections::HashMap,
    fmt::{Display, Formatter, Result as FmtResult},
    hash::{Hash, Hasher},
    path::PathBuf,
    result,
    sync::Arc,
};
use tokio::{
    sync::{Mutex, RwLock},
    task::JoinSet,
};
use zenoh::{handlers::FifoChannelHandler, pubsub::Subscriber, sample::Sample};

static SUCCESS_KEY_EXP: &str = "success";
static FAILURE_KEY_EXP: &str = "failure";
static INPUT_KEY_EXP: &str = "input_node/outputs";

#[derive(Serialize, Deserialize, Clone, Debug)]
enum NodeOutput {
    Packet(String, HashMap<String, PathSet>),
    ProcessingCompleted(String),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ProcessingFailure {
    node_id: String,
    error: String,
}

#[expect(
    clippy::type_complexity,
    reason = "too complex, but necessary for async handling"
)]
#[derive(Debug)]
struct PipelineRun {
    /// `PipelineJob` that this run is associated with
    pipeline_job: PipelineJob, // The pipeline job that this run is associated with
    node_tasks: JoinSet<Result<()>>, // JoinSet of tasks for each node in the pipeline
    outputs: Arc<RwLock<HashMap<String, Vec<HashMap<String, PathSet>>>>>, // String is the node key, while hash
}

impl PartialEq for PipelineRun {
    fn eq(&self, other: &Self) -> bool {
        self.pipeline_job.hash == other.pipeline_job.hash
    }
}

impl Eq for PipelineRun {}

impl Hash for PipelineRun {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.pipeline_job.hash.hash(state);
    }
}

impl Display for PipelineRun {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "PipelineRun({})", self.pipeline_job.hash)
    }
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
    pipeline_runs: HashMap<String, PipelineRun>,
}

/**
 * This is an implementation of a pipeline runner that uses Zenoh to communicate between the tasks
 * The runtime is tokio
 *
 * These are the key expressions of the components of the pipeline:
 * - Input Node: `pipeline_job_hash/input_node/outputs` (This is where the `pipeline_job` packets get fed to)
 * - Nodes: `pipeline_job_hash/node_id/outputs/(success|failure)` (This is where the node outputs are sent to)
*/
impl DockerPipelineRunner {
    /// Create a new Docker pipeline runner
    pub fn new() -> Self {
        Self::default()
    }

    /// # Errors
    /// Will error out if the pipeline job fails to start
    pub async fn start(
        &mut self,
        pipeline_job: PipelineJob,
        namespace: &str, // Name space to save pod_results to
        namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<String> {
        // Create a new pipeline run
        let mut pipeline_run = PipelineRun {
            pipeline_job,
            outputs: Arc::new(RwLock::new(HashMap::new())),
            node_tasks: JoinSet::new(),
        };

        // Get the pipeline_job_hash which will be use to identify the pipeline run
        let pipeline_job_hash = pipeline_run.pipeline_job.hash.clone();

        let graph = &pipeline_run.pipeline_job.pipeline.graph;

        // Create the subscriber to listen to node ready status before sending inputs
        let session = Arc::new(
            zenoh::open(zenoh::Config::default())
                .await
                .context(selector::AgentCommunicationFailure {})?,
        );

        let subscriber = session
            .declare_subscriber(format!("{pipeline_job_hash}/*/status/ready"))
            .await
            .context(selector::AgentCommunicationFailure {})?;

        // For each node, we will create call create_node_processing_task
        for node_idx in graph.node_indices() {
            let node = &graph[node_idx];

            // Spawn the task
            pipeline_run
                .node_tasks
                .spawn(Self::spawn_node_processing_task(
                    node.clone(),
                    pipeline_run.pipeline_job.pipeline.clone(),
                    pipeline_job_hash.clone(),
                    namespace.to_owned(),
                    namespace_lookup.clone(),
                    Arc::clone(&session),
                ));
        }

        // Spawn the task that captures the outputs from the output_nodes
        // For now the output nodes are hardcoded to be the leaf nodes of the pipeline
        for node in pipeline_run.pipeline_job.pipeline.get_leaf_nodes() {
            pipeline_run
                .node_tasks
                .spawn(Self::create_capture_task_for_node(
                    node.id.clone(),
                    pipeline_run.pipeline_job.hash.clone(),
                    Arc::clone(&pipeline_run.outputs),
                    Arc::clone(&session),
                ));
        }

        let num_of_nodes = graph.node_count();
        let mut ready_nodes = 0;

        // Wait for all nodes to be ready before sending inputs
        while (subscriber.recv_async().await).is_ok() {
            // Message is empty, just increment the counter
            ready_nodes += 1;

            if ready_nodes == num_of_nodes {
                break; // All nodes are ready, we can start sending inputs
            }
        }

        // Submit the input_packets to the correct key_exp
        let input_node_key_exp = format!("{pipeline_job_hash}/{INPUT_KEY_EXP}");
        for packet in &pipeline_run.pipeline_job.input_packets {
            // Send the packet to the input node key_exp
            session
                .put(
                    &input_node_key_exp,
                    serde_json::to_string(&NodeOutput::Packet(
                        "input_node".to_owned(),
                        packet.clone(),
                    ))?,
                )
                .await
                .context(selector::AgentCommunicationFailure {})?;
        }

        // Send the complete processing message for the input node
        session
            .put(
                input_node_key_exp,
                serde_json::to_string(&NodeOutput::ProcessingCompleted("input_node".to_owned()))?,
            )
            .await
            .context(selector::AgentCommunicationFailure {})?;

        // Insert into the list of pipeline runs
        self.pipeline_runs
            .insert(pipeline_job_hash.clone(), pipeline_run);

        Ok(pipeline_job_hash)
    }

    /// Given a pipeline run, wait for all its tasks to complete and return the `PipelineResult`
    ///
    /// # Errors
    /// Will error out if any of the pipeline tasks failed to join
    pub async fn get_result(&mut self, pipeline_run_id: &str) -> Result<PipelineResult> {
        // To get the result, the pipeline execution must be complete, so we need to await on the tasks

        let pipeline_run =
            self.pipeline_runs
                .get_mut(pipeline_run_id)
                .context(selector::KeyMissing {
                    key: pipeline_run_id.to_owned(),
                })?;

        // Wait for all the tasks to complete
        while let Some(result) = pipeline_run.node_tasks.join_next().await {
            match result {
                Ok(Ok(())) => {} // Task completed successfully
                Ok(Err(err)) => {
                    eprintln!("Task failed with err: {err}");
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
            output_packets: pipeline_run.outputs.read().await.clone(),
        })
    }

    /// Stop the pipeline run and all its tasks
    /// # Errors
    /// Will error out if the pipeline run is not found or if any of the tasks fail to stop correctly
    pub async fn stop(&mut self, pipeline_run_id: &str) -> Result<()> {
        // To stop the pipeline run, we need to send a stop message to all the tasks
        // Get the pipeline run first
        let pipeline_run =
            self.pipeline_runs
                .get_mut(pipeline_run_id)
                .context(selector::KeyMissing {
                    key: pipeline_run_id.to_owned(),
                })?;

        let session = zenoh::open(zenoh::Config::default())
            .await
            .context(selector::AgentCommunicationFailure {})?;

        // Send the stop message into the stop key_exp, the msg is just an empty vector
        session
            .put(
                format!("{}/stop", pipeline_run.pipeline_job.hash),
                Vec::new(),
            )
            .await
            .context(selector::AgentCommunicationFailure {})?;

        while pipeline_run.node_tasks.join_next().await.is_some() {}
        Ok(())
    }

    #[expect(clippy::type_complexity, reason = "Needed for async")]
    async fn create_capture_task_for_node(
        node_id: String,
        pipeline_run_id: String,
        outputs: Arc<RwLock<HashMap<String, Vec<HashMap<String, PathSet>>>>>,
        session: Arc<zenoh::Session>,
    ) -> Result<()> {
        // Create a zenoh session
        let subscriber = session
            .declare_subscriber(format!(
                "{pipeline_run_id}/{node_id}/outputs/{SUCCESS_KEY_EXP}"
            ))
            .await
            .context(selector::AgentCommunicationFailure {})?;

        while let Ok(payload) = subscriber.recv_async().await {
            // Extract the message from the payload
            let msg: NodeOutput = serde_json::from_slice(&payload.payload().to_bytes())?;

            match msg {
                NodeOutput::Packet(sender_id, hash_map) => {
                    // Store the output packet in the outputs map
                    let mut outputs_lock = outputs.write().await;
                    outputs_lock
                        .entry(node_id.clone())
                        .or_default()
                        .push(hash_map);
                }
                NodeOutput::ProcessingCompleted(_) => {
                    // Processing is completed, thus we can exit this task
                    break;
                }
            }
        }
        Ok(())
    }

    /// Function to start tasks associated with the node
    /// Steps:
    /// - Create the node processor based on the kernel type
    /// - Create the zenoh session
    /// - Create a join set to spawn and handle incoming messages tasks
    /// - Create a subscriber for each of the parent nodes (Should only be 1, unless it is a joiner node)
    /// - For each subscriber, handle the incoming message appropriately
    ///
    /// # Errors
    /// Will error out if the kernel for the node is not found or if the
    async fn spawn_node_processing_task(
        node: Node,
        pipeline: Pipeline,
        pipeline_job_id: String,
        namespace: String,
        namespace_lookup: HashMap<String, PathBuf>,
        session: Arc<zenoh::Session>,
    ) -> Result<()> {
        // Create the correct processor for the node based on the kernel type
        let node_processor: Arc<Mutex<Box<dyn NodeProcessor>>> = Arc::new(Mutex::new(
            match get(&pipeline.kernel_lut, &node.kernel_hash)? {
                Kernel::Pod(pod) => Box::new(PodProcessor::new(Arc::clone(pod))),
                Kernel::Mapper(mapper) => Box::new(MapperProcessor::new(Arc::clone(mapper))),
                Kernel::Joiner => {
                    // Need to get the parent node id for this joiner node
                    let parent_nodes_id = pipeline
                        .get_parents_for_node(&node)
                        .map(|parent_node| parent_node.id.clone())
                        .collect::<Vec<_>>();
                    Box::new(JoinerProcessor::new(parent_nodes_id))
                }
            },
        ));

        // Create a join set to spawn and handle incoming messages tasks
        let mut listener_tasks = JoinSet::new();

        // Create the list of key_expressions to subscribe to
        let mut key_exps_to_subscribe_to = pipeline
            .get_parents_for_node(&node)
            .map(|parent_node| {
                format!(
                    "{pipeline_job_id}/{}/outputs/{SUCCESS_KEY_EXP}",
                    parent_node.id
                )
            })
            .collect::<Vec<_>>();

        // If there was no parent node, then this is root node, therefore we need to subscribe to the input node
        if key_exps_to_subscribe_to.is_empty() {
            key_exps_to_subscribe_to.push(format!("{pipeline_job_id}/{INPUT_KEY_EXP}"));
        }

        // Create a subscriber for each of the parent nodes (Should only be 1, unless it is a joiner node)
        for key_exp in &key_exps_to_subscribe_to {
            let subscriber = session
                .declare_subscriber(key_exp)
                .await
                .context(selector::AgentCommunicationFailure {})?;

            listener_tasks.spawn(Self::start_async_processor_task(
                subscriber,
                Arc::clone(&node_processor),
                node.id.clone(),
                pipeline_job_id.clone(),
                namespace.clone(),
                namespace_lookup.clone(),
                Arc::clone(&session),
            ));
        }

        // Create the listener task for the stop request
        let mut stop_listener_task = JoinSet::new();

        stop_listener_task.spawn(Self::start_stop_request_task(
            Arc::clone(&node_processor),
            format!("{pipeline_job_id}/{}/stop", node.id),
            Arc::clone(&session),
        ));

        // Wait for all tasks to be spawned and reply with ready message
        // This is to ensure that the pipeline run knows when all tasks are ready to receive inputs

        let mut num_of_ready_subcribers: usize = 0;
        // Build the subscriber
        let status_subscriber = session
            .declare_subscriber(format!(
                "{pipeline_job_id}/{}/subscriber/status/ready",
                node.id
            ))
            .await
            .context(selector::AgentCommunicationFailure {})?;

        while status_subscriber.recv_async().await.is_ok() {
            num_of_ready_subcribers += 1;
            if num_of_ready_subcribers == key_exps_to_subscribe_to.len() {
                // +1 for the stop request task
                break; // All tasks are ready, we can start sending inputs
            }
        }

        // Send a ready message so the pipeline knows when to start sending inputs
        session
            .put(
                format!("{pipeline_job_id}/{}/status/ready", node.id),
                &node.id,
            )
            .await
            .context(selector::AgentCommunicationFailure {})?;

        // Wait for all task to complete
        listener_tasks.join_all().await;

        // Abort the stop listener task since we don't need it anymore
        stop_listener_task.abort_all();

        Ok(())
    }

    async fn start_async_processor_task(
        subscriber: Subscriber<FifoChannelHandler<Sample>>,
        node_processor: Arc<Mutex<Box<dyn NodeProcessor>>>,
        node_id: String,
        pipeline_job_id: String,
        namespace: String,
        namespace_lookup: HashMap<String, PathBuf>,
        session: Arc<zenoh::Session>,
    ) -> Result<()> {
        // We do not know when tokio will start executing this task, therefore we need to send a ready message
        // back to our spawner task
        session
            .put(
                format!("{pipeline_job_id}/{node_id}/subscriber/status/ready"),
                &node_id,
            )
            .await
            .context(selector::AgentCommunicationFailure {})?;

        while let Ok(payload) = subscriber.recv_async().await {
            // Extract the message from the payload
            match serde_json::from_slice(&payload.payload().to_bytes())? {
                NodeOutput::Packet(sender_id, hash_map) => {
                    // Process the packet using the node processor
                    node_processor.lock().await.process_packet(
                        &sender_id,
                        &node_id,
                        &hash_map,
                        Arc::clone(&session),
                        &format!("{}/{}/outputs", pipeline_job_id, node_id.clone()),
                        &namespace,
                        &namespace_lookup,
                    )?;
                }
                NodeOutput::ProcessingCompleted(sender_id) => {
                    // Notify the processor that the parent node has completed processing
                    if node_processor
                        .lock()
                        .await
                        .mark_parent_as_complete(&sender_id)
                        .await
                    {
                        // This was the last parent, thus we need to send the processing complete message
                        let output_key_exp =
                            format!("{pipeline_job_id}/{node_id}/outputs/{SUCCESS_KEY_EXP}");
                        session
                            .put(
                                output_key_exp,
                                serde_json::to_string(&NodeOutput::ProcessingCompleted(
                                    node_id.clone(),
                                ))?,
                            )
                            .await
                            .context(selector::AgentCommunicationFailure {})?;
                    }
                    break;
                }
            }
        }

        Ok::<(), OrcaError>(())
    }

    async fn start_stop_request_task(
        node_processor: Arc<Mutex<Box<dyn NodeProcessor>>>,
        pipeline_run_id: String,
        session: Arc<zenoh::Session>,
    ) -> Result<()> {
        let subscriber = session
            .declare_subscriber(pipeline_run_id.clone() + "/stop")
            .await
            .context(selector::AgentCommunicationFailure {})?;
        while subscriber.recv_async().await.is_ok() {
            // Received a requst to stop, therefore we need to tell the node_processor to shutdown
            node_processor.lock().await.stop();
        }
        Ok::<(), OrcaError>(())
    }
}

/// Unify the interface for node processors and provide a common way to handle processing of incoming messages
/// This trait defines the methods that all node processors should implement
///
/// Main purpose was to reduce the amount of code duplication between different node processors
/// As a result, each processor only needs to worry about writing their own function to process the msg
#[async_trait]
trait NodeProcessor: Send + Sync {
    fn process_packet(
        &mut self,
        sender_node_id: &str,
        node_id: &str,
        packet: &HashMap<String, PathSet>,
        session: Arc<zenoh::Session>,
        output_key_exp: &str,
        namespace: &str,
        namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<()>;

    /// Notifies the processor that the parent node has completed processing
    /// If the parent node was the last one to complete, this function will wait till all task are done
    /// and send the node processing complete message then return.
    ///
    /// Otherwise it will return immediately
    ///
    /// # Returns
    /// true if the parent node was the last one to complete processing, user send
    /// the processing completion message to the output
    ///
    /// false if there are still other parent nodes that need to complete processing
    async fn mark_parent_as_complete(&mut self, parent_node_id: &str) -> bool;

    fn stop(&mut self);
}

/// Processor for Pods
/// Currently missing implementation to call agents for actual pod processing
struct PodProcessor {
    pod: Arc<Pod>,
    processing_tasks: JoinSet<Result<(), OrcaError>>,
}

impl PodProcessor {
    fn new(pod: Arc<Pod>) -> Self {
        Self {
            pod,
            processing_tasks: JoinSet::new(),
        }
    }
}

#[async_trait]
impl NodeProcessor for PodProcessor {
    #[expect(
        clippy::unwrap_used,
        clippy::unwrap_in_result,
        reason = "Hard code for now, will be replaced by agent"
    )]
    fn process_packet(
        &mut self,
        _sender_node_id: &str,
        node_id: &str,
        packet: &HashMap<String, PathSet>,
        session: Arc<zenoh::Session>,
        output_key_exp: &str,
        namespace: &str,
        namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<()> {
        // Process the packet using the pod
        // Create the pod_job

        // We need a unique hash for this given input packet process by the node
        // therefore we need to generate a hash that has the pod_id + input_packet
        let node_id_bytes = node_id.as_bytes().to_vec();
        let packet_copy = packet.clone();
        let input_packet_hash = {
            let mut buf = node_id_bytes;
            let mut serializer = Serializer::new(&mut buf);
            serialize_hashmap(&packet_copy, &mut serializer)?;
            hash_buffer(buf)
        };
        let output_dir = URI {
            namespace: namespace.to_owned(),
            path: PathBuf::from(format!("pod_runs/{}/{}", self.pod.hash, input_packet_hash)),
        };

        let cpu_limit = self.pod.recommended_cpus;
        let memory_limit = self.pod.recommended_memory;

        // Create the pod job
        let pod_job = PodJob::new(
            None,
            Arc::clone(&self.pod),
            packet.clone(),
            output_dir,
            cpu_limit,
            memory_limit,
            None,
            namespace_lookup,
        )?;

        // Simulate pod execution by just printing out pod_job_hash and pod hash
        // This will be replaced by sending the pod_job to the orchestrator via the agent

        // Build the output_packet, in reality, this will be extracted from the pod_result

        let output_packet = self
            .pod
            .output_spec
            .keys()
            .map(|output_key| (output_key.clone(), packet.values().next().cloned().unwrap()))
            .collect::<HashMap<_, _>>();

        let node_id_clone = node_id.to_owned();
        let output_key_exp_clone = output_key_exp.to_owned();
        self.processing_tasks.spawn(async move {
            // For now we will just send the input_packet to the success channel
            session
                .put(
                    output_key_exp_clone + "/" + SUCCESS_KEY_EXP,
                    serde_json::to_string(&NodeOutput::Packet(node_id_clone, output_packet))?,
                )
                .await
                .context(selector::AgentCommunicationFailure {})?;

            Ok(())
        });
        Ok(())
    }

    async fn mark_parent_as_complete(&mut self, _parent_node_id: &str) -> bool {
        // For pod we only have one parent, thus execute the exit case
        while let Some(result) = self.processing_tasks.join_next().await {
            match result {
                Ok(Ok(())) => {}
                Ok(Err(err)) => {}
                Err(err) => {}
            }
        }
        true
    }

    fn stop(&mut self) {
        self.processing_tasks.abort_all();
    }
}

/// Processor for Mapper nodes
/// This processor renames the `input_keys` from the input packet to the `output_keys` defined by the map
struct MapperProcessor {
    mapper: Arc<Mapper>,
    processing_tasks: JoinSet<Result<(), OrcaError>>,
}

impl MapperProcessor {
    fn new(mapper: Arc<Mapper>) -> Self {
        Self {
            mapper,
            processing_tasks: JoinSet::new(),
        }
    }
}

#[async_trait]
impl NodeProcessor for MapperProcessor {
    fn process_packet(
        &mut self,
        _sender_node_id: &str,
        node_id: &str,
        packet: &HashMap<String, PathSet>,
        session: Arc<zenoh::Session>,
        output_key_exp: &str,
        _namespace: &str,
        _namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<()> {
        let mapping = self.mapper.mapping.clone();
        let packet_clone = packet.clone();
        let node_id_clone = node_id.to_owned();
        let output_key_exp_clone = output_key_exp.to_owned();

        self.processing_tasks.spawn(async move {
            let result = {
                // Apply the mapping to the input packet
                let output_map = mapping
                    .iter()
                    .map(|(input_key, output_key)| {
                        let input = get(&packet_clone, input_key)?.clone();
                        Ok((output_key.to_owned(), input))
                    })
                    .collect::<Result<HashMap<_, _>>>()?;

                // Send the packet outwards
                session
                    .put(
                        format!("{}/{}", output_key_exp_clone, SUCCESS_KEY_EXP),
                        &serde_json::to_string(&NodeOutput::Packet(
                            node_id_clone.clone(),
                            output_map,
                        ))?,
                    )
                    .await
                    .context(selector::AgentCommunicationFailure {})?;
                Ok::<(), OrcaError>(())
            };

            if let Err(err) = result {
                // If there was an error, we send it to the failure channel
                session
                    .put(
                        format!("{}/{}", output_key_exp_clone, FAILURE_KEY_EXP),
                        serde_json::to_string(&ProcessingFailure {
                            node_id: node_id_clone.clone(),
                            error: err.to_string(),
                        })?,
                    )
                    .await
                    .context(selector::AgentCommunicationFailure {})?;
            }
            Ok(())
        });
        Ok(())
    }

    async fn mark_parent_as_complete(&mut self, _parent_node_id: &str) -> bool {
        // For mapper we only have one parent, thus execute the exit case
        while (self.processing_tasks.join_next().await).is_some() {
            // Wait for all tasks to complete
        }

        true
    }

    fn stop(&mut self) {
        self.processing_tasks.abort_all();
    }
}

/// Processor for Joiner nodes
/// This processor combines packets from multiple parent nodes into a single output packet
/// It uses a cartesian product to combine packets from different parents
struct JoinerProcessor {
    /// Cache for all packets received by the node
    input_packet_cache: HashMap<String, Vec<HashMap<String, PathSet>>>,
    completed_parents: Vec<String>,
    processing_tasks: JoinSet<Result<(), OrcaError>>,
}

impl JoinerProcessor {
    fn new(parents_node_id: Vec<String>) -> Self {
        let input_packet_cache = parents_node_id
            .into_iter()
            .map(|id| (id, Vec::new()))
            .collect();
        Self {
            input_packet_cache,
            completed_parents: Vec::new(),
            processing_tasks: JoinSet::new(),
        }
    }

    fn compute_cartesian_product(
        factors: &[Vec<HashMap<String, PathSet>>],
    ) -> Vec<HashMap<String, PathSet>> {
        factors
            .iter()
            .multi_cartesian_product()
            .map(|packets_to_combined| {
                packets_to_combined
                    .into_iter()
                    .fold(HashMap::new(), |mut acc, packet| {
                        acc.extend(packet.clone());
                        acc
                    })
            })
            .collect::<Vec<_>>()
    }
}

#[async_trait]
impl NodeProcessor for JoinerProcessor {
    fn process_packet(
        &mut self,
        sender_node_id: &str,
        node_id: &str,
        packet: &HashMap<String, PathSet>,
        session: Arc<zenoh::Session>,
        output_key_exp: &str,
        _namespace: &str,
        _namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<()> {
        self.input_packet_cache
            .get_mut(sender_node_id)
            .context(selector::KeyMissing {
                key: sender_node_id.to_owned(),
            })?
            .push(packet.clone());

        // Check if we have all the other parents needed to compute the cartesian product
        if self.input_packet_cache.values().all(|v| !v.is_empty()) {
            // Get all the cached packets from other parents
            let other_parent_ids = self
                .input_packet_cache
                .keys()
                .filter(|key| *key != sender_node_id);

            // Build the factors of the product as owned values to avoid lifetime issues
            let mut factors = other_parent_ids
                .map(|id| get(&self.input_packet_cache, id).cloned())
                .collect::<Result<Vec<_>>>()?;

            // Add the new packet as a factor
            factors.push(vec![packet.clone()]);

            // Compute the cartesian product of the factors
            let node_id_clone = node_id.to_owned();
            let output_key_exp_clone = output_key_exp.to_owned();

            self.processing_tasks.spawn(async move {
                // Convert Vec<Vec<HashMap<...>>> to Vec<&Vec<HashMap<...>>> for compute_cartesian_product
                let cartesian_product = Self::compute_cartesian_product(&factors);
                // Post all products to the output channel
                for output_packet in cartesian_product {
                    let result = {
                        session
                            .put(
                                format!("{}/{}", output_key_exp_clone, SUCCESS_KEY_EXP),
                                serde_json::to_string(&NodeOutput::Packet(
                                    node_id_clone.clone(),
                                    output_packet,
                                ))?,
                            )
                            .await
                            .context(selector::AgentCommunicationFailure {})?;
                        Ok::<(), OrcaError>(())
                    };

                    // If the result is an error, we will just send it to the error channel
                    if let Err(err) = result {
                        session
                            .put(
                                format!("{}/{}", output_key_exp_clone, FAILURE_KEY_EXP),
                                serde_json::to_string(&ProcessingFailure {
                                    node_id: node_id_clone.clone(),
                                    error: err.to_string(),
                                })?,
                            )
                            .await
                            .context(selector::AgentCommunicationFailure {})?;
                    }
                }

                Ok(())
            });
        }
        Ok(())
    }

    async fn mark_parent_as_complete(&mut self, _parent_node_id: &str) -> bool {
        // For Joiner, we need to determine if all parents are complete, if so then wait for task to complete
        // before returning true
        self.completed_parents.push(_parent_node_id.to_owned());

        // If we have all parents completed, we can wait for the tasks to complete
        if self.completed_parents.len() == self.input_packet_cache.len() {
            while (self.processing_tasks.join_next().await).is_some() {
                // Wait for all tasks to complete
            }
            return true;
        }

        // If not all parents are completed, we return false
        false
    }

    fn stop(&mut self) {
        // We want to abort any computation
        self.processing_tasks.abort_all();
    }
}

#[cfg(test)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[expect(clippy::panic_in_result_fn, reason = "Unit test")]
async fn joiner() -> Result<()> {
    // let parent_ids = vec!["0".to_owned(), "1".to_owned(), "2".to_owned()];

    // let mut joiner_process = JoinerProcessor::new(parent_ids);

    // // Make each parent has 1 packet
    // for idx in 0..2 {
    //     let packet = make_test_packet(format!("data_{idx}.txt").into());
    //     joiner_process.process_packet(idx, "joiner", packet, session, output_key_exp, namespace, namespace_lookup);
    // }

    // // Confirm that there should be no output yet

    // // Now we send the missing parent package
    // // This will yield one unique combination
    // joiner_process
    //     .process_packet("2", make_test_packet("data_1.txt".to_owned().into()))
    //     .await?;

    // // Confirm that the output is sent to the child channel
    // assert!(
    //     child_rx.len() == 1,
    //     "Should have only one message in the channel",
    // );
    // assert!(
    //     child_rx.recv().await.is_some(),
    //     "Should have received a message"
    // );

    // // Insert another one
    // joiner_process
    //     .process_packet("2", make_test_packet("data_2.txt".to_owned().into()))
    //     .await?;

    // // The joiner node should send another one
    // assert!(
    //     child_rx.len() == 1,
    //     "Should have only one message in the channel",
    // );
    // assert!(
    //     child_rx.recv().await.is_some(),
    //     "Should have received a message"
    // );

    // // Now insert to packet for parent 0, which should yield 2 packets in total
    // // This is because of the cartesian product
    // joiner_process
    //     .process_packet("0", make_test_packet("data_2.txt".to_owned().into()))
    //     .await?;

    // assert!(
    //     child_rx.len() == 2,
    //     "Should have only two messages in the channel",
    // );
    // assert!(
    //     child_rx.recv().await.is_some(),
    //     "Should have received a message"
    // );

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
