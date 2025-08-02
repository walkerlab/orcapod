use crate::{
    core::{
        crypto::hash_buffer,
        model::{pipeline::PipelineNode, serialize_hashmap},
        util::get,
    },
    uniffi::{
        error::{Kind, OrcaError, Result, selector},
        model::{
            packet::{PathSet, URI},
            pipeline::{Kernel, Mapper, Pipeline, PipelineJob, PipelineResult},
            pod::{Pod, PodJob, PodResult, PodResultStatus},
        },
        orchestrator::{
            agent::{Agent, AgentClient, Response},
            docker::LocalDockerOrchestrator,
        },
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

/// Internal representation of a pipeline run, this should not be made public due to the fact that it contains
/// internal states and tasks
#[derive(Debug)]
struct PipelineRun {
    /// `PipelineJob` that this run is associated with
    pipeline_job: Arc<PipelineJob>, // The pipeline job that this run is associated with
    node_tasks: JoinSet<Result<()>>, // JoinSet of tasks for each node in the pipeline
    outputs: Arc<RwLock<HashMap<String, Vec<PathSet>>>>, // String is the node key, while hash
    orchestrator_agent: Arc<Agent>, // This is placed in pipeline due to the current design requiring a namespace to operate on
    orchestrator_agent_task: JoinSet<Result<()>>, // JoinSet of tasks for the orchestrator agent
    failure_logs: Arc<RwLock<Vec<ProcessingFailure>>>, // Logs of processing failures
    failure_logging_task: JoinSet<Result<()>>, // JoinSet of tasks for logging failures
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

/// Runner that uses a docker agent to run pipelines
pub struct DockerPipelineRunner {
    /// User label on which group of agents this runner is associated with
    pub group: String,
    /// The host name of the runner
    pub host: String,
    pipeline_runs: HashMap<String, PipelineRun>,
}

/// This is an implementation of a pipeline runner that uses Zenoh to communicate between the tasks
/// The runtime is tokio
///
/// These are the key expressions of the components of the pipeline:
/// Input Node: `pipeline_job_hash/input_node/outputs` (This is where the `pipeline_job` packets get fed to)
/// Nodes: `pipeline_job_hash/node_id/outputs/(success|failure)` (This is where the node outputs are sent to)
///
impl DockerPipelineRunner {
    /// Create a new Docker pipeline runner
    /// # Errors
    /// Will error out if the environment variable `HOSTNAME` is not set
    pub fn new(group: String) -> Result<Self> {
        let host = hostname::get()?.to_string_lossy().to_string();
        Ok(Self {
            group,
            host,
            pipeline_runs: HashMap::new(),
        })
    }

    /// Will start a new pipeline run with the given `PipelineJob`
    /// This will start the async tasks for each node in the pipeline
    /// including the one that captures the outputs from the leaf nodes
    ///
    /// Upon receiving the ready message from all the nodes, it will send the input packets to the input node
    ///
    /// # Errors
    /// Will error out if the pipeline job fails to start
    pub async fn start(
        &mut self,
        pipeline_job: PipelineJob,
        namespace: &str, // Name space to save pod_results to
        namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<String> {
        // Create the orchestrator
        let orchestrator_agent = Agent::new(
            self.group.clone(),
            self.host.clone(),
            LocalDockerOrchestrator::new()?.into(),
        )?;

        // Create a new pipeline run
        let mut pipeline_run = PipelineRun {
            pipeline_job: pipeline_job.into(),
            outputs: Arc::new(RwLock::new(HashMap::new())),
            node_tasks: JoinSet::new(),
            orchestrator_agent: orchestrator_agent.into(),
            orchestrator_agent_task: JoinSet::new(),
            failure_logs: Arc::new(RwLock::new(Vec::new())),
            failure_logging_task: JoinSet::new(),
        };

        // Get the preexisting zenoh session from agent
        let session = Arc::clone(&pipeline_run.orchestrator_agent.client.session);

        // Spawn task for each of the processing node
        let orchestrator_agent_clone = Arc::clone(&pipeline_run.orchestrator_agent);
        let namespace_lookup_clone = namespace_lookup.clone();
        // Start the orchestrator agent service
        pipeline_run.orchestrator_agent_task.spawn(async move {
            orchestrator_agent_clone
                .start(&namespace_lookup_clone, None)
                .await
        });

        // Create failure logging task
        pipeline_run
            .failure_logging_task
            .spawn(Self::failure_capture_task(
                Arc::clone(&session),
                Arc::clone(&pipeline_run.failure_logs),
            ));

        // Create the processor task for each node
        // The id for the pipeline_run is the pipeline_job hash
        let pipeline_run_id = pipeline_run.pipeline_job.hash.clone();

        let graph = &pipeline_run.pipeline_job.pipeline.graph;

        // Create the subscriber that listen for ready messages
        let subscriber = session
            .declare_subscriber(self.get_base_key_exp(&pipeline_run_id) + "/*/status/ready")
            .await
            .context(selector::AgentCommunicationFailure {})?;

        // Get the set of input_nodes
        let input_nodes = pipeline_run.pipeline_job.pipeline.get_input_nodes();

        // Iterate through each node in the graph and spawn a task for each
        for node_idx in graph.node_indices() {
            let node = &graph[node_idx];

            // Spawn the task
            pipeline_run
                .node_tasks
                .spawn(Self::spawn_node_processing_task(
                    node.clone(),
                    Arc::clone(&pipeline_run.pipeline_job.pipeline),
                    input_nodes.contains(&node.name),
                    self.get_base_key_exp(&pipeline_run_id),
                    namespace.to_owned(),
                    namespace_lookup.clone(),
                    Arc::clone(&session),
                    Arc::clone(&pipeline_run.orchestrator_agent.client),
                ));
        }

        // Spawn the task that captures the outputs based on the output_spec
        let mut node_output_spec = HashMap::new();
        // Group the output spec by node
        for (output_key, node_uri) in &pipeline_run.pipeline_job.pipeline.output_spec {
            node_output_spec
                .entry(node_uri.node_name.clone())
                .or_insert_with(HashMap::new)
                .insert(output_key.clone(), node_uri.key.clone());
        }

        for (node_id, key_mapping) in node_output_spec {
            // Create the key expression to subscribe to
            let key_exp_to_sub = format!(
                "{}/{}/outputs/{}",
                self.get_base_key_exp(&pipeline_run_id),
                node_id,
                SUCCESS_KEY_EXP,
            );

            // Spawn the task that captures the outputs
            pipeline_run
                .node_tasks
                .spawn(Self::create_output_capture_task_for_node(
                    key_mapping,
                    Arc::clone(&pipeline_run.outputs),
                    Arc::clone(&session),
                    key_exp_to_sub,
                ));
        }

        // Wait for all nodes to be ready before sending inputs
        let num_of_nodes = graph.node_count();
        let mut ready_nodes = 0;

        while (subscriber.recv_async().await).is_ok() {
            // Message is empty, just increment the counter
            ready_nodes += 1;

            if ready_nodes == num_of_nodes {
                break; // All nodes are ready, we can start sending inputs
            }
        }

        // Submit the input_packets to the correct key_exp
        let base_input_node_key_exp = format!(
            "{}/{}",
            self.get_base_key_exp(&pipeline_run_id),
            INPUT_KEY_EXP,
        );

        // For each node send all the packets associate with it
        for (node_name, input_packets) in pipeline_run.pipeline_job.get_input_packet_per_node()? {
            for packet in input_packets {
                // Send the packet to the input node key_exp
                let output_key_exp = format!("{base_input_node_key_exp}/{node_name}");
                session
                    .put(
                        &output_key_exp,
                        serde_json::to_string(&NodeOutput::Packet(
                            "input_node".to_owned(),
                            packet.clone(),
                        ))?,
                    )
                    .await
                    .context(selector::AgentCommunicationFailure {})?;

                // All packets associate with node are sent, we can send processing complete msg now
                session
                    .put(
                        &output_key_exp,
                        serde_json::to_string(&NodeOutput::ProcessingCompleted(
                            "input_node".to_owned(),
                        ))?,
                    )
                    .await
                    .context(selector::AgentCommunicationFailure {})?;
            }
        }

        // Insert into the list of pipeline runs
        self.pipeline_runs
            .insert(pipeline_run_id.clone(), pipeline_run);

        // Return the pipeline run id
        Ok(pipeline_run_id)
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

        // Figure out how to do this later
        Ok(PipelineResult {
            pipeline_job: Arc::clone(&pipeline_run.pipeline_job),
            output_packets: pipeline_run.outputs.read().await.clone(),
        })
    }

    /// Stop the pipeline run and all its tasks
    /// This will send a stop message to a channel that all node manager task are subscribed to.
    /// Upon receiving the stop message, each node manager will force abort all of its task and exit.
    ///
    /// # Errors
    /// Will error out if the pipeline run is not found or if any of the tasks fail to stop correctly
    pub async fn stop(&mut self, pipeline_run_id: &str) -> Result<()> {
        let stop_key_exp = format!(
            "{}/{}/stop",
            self.get_base_key_exp(pipeline_run_id),
            pipeline_run_id
        );
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
            .put(stop_key_exp, Vec::new())
            .await
            .context(selector::AgentCommunicationFailure {})?;

        while pipeline_run.node_tasks.join_next().await.is_some() {}
        Ok(())
    }

    /// This will capture the outputs of the given nodes and store it in the `outputs` map
    async fn create_output_capture_task_for_node(
        //<Key to pull from the node, Key that will be mapped to in the outputs>
        key_mapping: HashMap<String, String>,
        outputs: Arc<RwLock<HashMap<String, Vec<PathSet>>>>,
        session: Arc<zenoh::Session>,
        key_exp_to_sub: String,
    ) -> Result<()> {
        // Determine which keys we are interested in for the given node_id

        // Create a zenoh session
        let subscriber = session
            .declare_subscriber(key_exp_to_sub)
            .await
            .context(selector::AgentCommunicationFailure {})?;

        while let Ok(payload) = subscriber.recv_async().await {
            // Extract the message from the payload
            let msg: NodeOutput = serde_json::from_slice(&payload.payload().to_bytes())?;

            match msg {
                NodeOutput::Packet(_, packet) => {
                    // Figure out which keys
                    // Store the output packet in the outputs map
                    let mut outputs_lock = outputs.write().await;
                    for (output_key, node_key) in &key_mapping {
                        outputs_lock
                            .entry(output_key.to_owned())
                            .or_default()
                            .push(get(&packet, node_key.as_str())?.clone());
                    }
                }
                NodeOutput::ProcessingCompleted(_) => {
                    // Processing is completed, thus we can exit this task
                    break;
                }
            }
        }
        Ok(())
    }

    async fn failure_capture_task(
        session: Arc<zenoh::Session>,
        failure_logs: Arc<RwLock<Vec<ProcessingFailure>>>,
    ) -> Result<()> {
        let sub = session
            .declare_subscriber(format!("**/outputs/{FAILURE_KEY_EXP}"))
            .await
            .context(selector::AgentCommunicationFailure {})?;

        // Listen to any failure messages and write it the logs
        while let Ok(payload) = sub.recv_async().await {
            // Extract the message from the payload
            let process_failure: ProcessingFailure =
                serde_json::from_slice(&payload.payload().to_bytes())?;
            // Store the failure message in the logs
            failure_logs.write().await.push(process_failure.clone());
            if let Some(first_line) = process_failure.error.lines().next() {
                println!(
                    "Node {} processing failed with error: {}",
                    process_failure.node_id, first_line
                );
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
    /// - Create an abort listener task that will listen for stop requests
    /// - For each subscriber, handle the incoming message appropriately
    ///
    /// # Errors
    /// Will error out if the kernel for the node is not found or if the
    async fn spawn_node_processing_task(
        node: PipelineNode,
        pipeline: Arc<Pipeline>,
        is_input_node: bool,
        base_key_exp: String,
        namespace: String,
        namespace_lookup: HashMap<String, PathBuf>,
        session: Arc<zenoh::Session>,
        client: Arc<AgentClient>,
    ) -> Result<()> {
        // Create the correct processor for the node based on the kernel type
        let node_processor: Arc<Mutex<Box<dyn NodeProcessor>>> =
            Arc::new(Mutex::new(match &node.kernel {
                Kernel::Pod { pod } => Box::new(PodProcessor::new(Arc::clone(pod), client)),
                Kernel::Mapper { mapper } => Box::new(MapperProcessor::new(Arc::clone(mapper))),
                Kernel::Joiner => {
                    // Need to get the parent node id for this joiner node
                    let mut parent_nodes = pipeline
                        .get_node_parents(&node)
                        .map(|parent_node| parent_node.name.clone())
                        .collect::<Vec<_>>();

                    // Check if it this node takes input from input_nodes, if so we need ot add it to parent_node
                    if is_input_node {
                        parent_nodes.push("input_node".to_owned());
                    }

                    Box::new(JoinerProcessor::new(parent_nodes))
                }
            }));

        // Create a join set to spawn and handle incoming messages tasks
        let mut listener_tasks = JoinSet::new();

        // Create the list of key_expressions to subscribe to
        let mut key_exps_to_subscribe_to = pipeline
            .get_node_parents(&node)
            .map(|parent_node| {
                format!(
                    "{base_key_exp}/{}/outputs/{SUCCESS_KEY_EXP}",
                    parent_node.name
                )
            })
            .collect::<Vec<_>>();

        // Check if node is an input_node, if so we need to add the input node key expression
        if is_input_node {
            key_exps_to_subscribe_to
                .push(format!("{base_key_exp}/input_node/outputs/{}", node.name));
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
                node.name.clone(),
                base_key_exp.clone(),
                namespace.clone(),
                namespace_lookup.clone(),
                Arc::clone(&session),
            ));
        }

        // Create the listener task for the stop request
        let mut stop_listener_task = JoinSet::new();

        stop_listener_task.spawn(Self::start_stop_request_task(
            Arc::clone(&node_processor),
            format!("{base_key_exp}/{}/stop", node.name),
            Arc::clone(&session),
        ));

        // Wait for all tasks to be spawned and reply with ready message
        // This is to ensure that the pipeline run knows when all tasks are ready to receive inputs

        let mut num_of_ready_subscribers: usize = 0;
        // Build the subscriber
        let status_subscriber = session
            .declare_subscriber(format!(
                "{base_key_exp}/{}/subscriber/status/ready",
                node.name
            ))
            .await
            .context(selector::AgentCommunicationFailure {})?;

        while status_subscriber.recv_async().await.is_ok() {
            num_of_ready_subscribers += 1;
            if num_of_ready_subscribers == key_exps_to_subscribe_to.len() {
                // +1 for the stop request task
                break; // All tasks are ready, we can start sending inputs
            }
        }

        // Send a ready message so the pipeline knows when to start sending inputs
        session
            .put(
                format!("{base_key_exp}/{}/status/ready", node.name),
                &node.name,
            )
            .await
            .context(selector::AgentCommunicationFailure {})?;

        // Wait for all task to complete
        listener_tasks.join_all().await;

        // Abort the stop listener task since we don't need it anymore
        stop_listener_task.abort_all();

        Ok(())
    }

    /// This is the actual handler for incoming messages for the node
    async fn start_async_processor_task(
        subscriber: Subscriber<FifoChannelHandler<Sample>>,
        node_processor: Arc<Mutex<Box<dyn NodeProcessor>>>,
        node_name: String,
        base_key_exp: String,
        namespace: String,
        namespace_lookup: HashMap<String, PathBuf>,
        session: Arc<zenoh::Session>,
    ) -> Result<()> {
        // We do not know when tokio will start executing this task, therefore we need to send a ready message
        // back to our spawner task
        session
            .put(
                format!("{base_key_exp}/{node_name}/subscriber/status/ready"),
                &node_name,
            )
            .await
            .context(selector::AgentCommunicationFailure {})?;

        let node_base_output_key_exp = format!("{base_key_exp}/{node_name}/outputs");
        while let Ok(payload) = subscriber.recv_async().await {
            // Extract the message from the payload
            match serde_json::from_slice(&payload.payload().to_bytes())? {
                NodeOutput::Packet(sender_id, hash_map) => {
                    // Process the packet using the node processor
                    let result = node_processor.lock().await.process_packet(
                        &sender_id,
                        &node_name,
                        &hash_map,
                        Arc::clone(&session),
                        &node_base_output_key_exp,
                        &namespace,
                        &namespace_lookup,
                    );

                    if let Err(err) = result {
                        try_to_forward_err_msg(
                            Arc::clone(&session),
                            err,
                            &node_base_output_key_exp,
                            &node_name,
                        )
                        .await;
                    }
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
                            format!("{base_key_exp}/{node_name}/outputs/{SUCCESS_KEY_EXP}");
                        session
                            .put(
                                output_key_exp,
                                serde_json::to_string(&NodeOutput::ProcessingCompleted(
                                    node_name.clone(),
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

    /// This task will listen for stop requests on the given key expression
    async fn start_stop_request_task(
        node_processor: Arc<Mutex<Box<dyn NodeProcessor>>>,
        base_key_exp: String,
        session: Arc<zenoh::Session>,
    ) -> Result<()> {
        let subscriber = session
            .declare_subscriber(format!("{base_key_exp}/stop"))
            .await
            .context(selector::AgentCommunicationFailure {})?;
        while subscriber.recv_async().await.is_ok() {
            // Received a request to stop, therefore we need to tell the node_processor to shutdown
            node_processor.lock().await.stop();
        }
        Ok::<(), OrcaError>(())
    }

    fn get_base_key_exp(&self, pipeline_run_id: &str) -> String {
        format!("{}/{}/{}", self.group, self.host, pipeline_run_id)
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
        base_output_key_exp: &str,
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

/// Util function to handle forwarding error messages to the failure channel
async fn try_to_forward_err_msg(
    session: Arc<zenoh::Session>,
    err: OrcaError,
    node_base_output_key_exp: &str,
    node_id: &str,
) {
    match async {
        session
            .put(
                format!("{node_base_output_key_exp}/{FAILURE_KEY_EXP}"),
                serde_json::to_string(&ProcessingFailure {
                    node_id: node_id.to_owned(),
                    error: err.to_string(),
                })?,
            )
            .await
            .context(selector::AgentCommunicationFailure {})?;
        Ok::<(), OrcaError>(())
    }
    .await
    {
        Ok(()) => {}
        Err(send_err) => {
            eprintln!("Failed to send failure message: {send_err}");
        }
    }
}

/// Processor for Pods
/// Currently missing implementation to call agents for actual pod processing
struct PodProcessor {
    pod: Arc<Pod>,
    processing_tasks: JoinSet<()>,
    client: Arc<AgentClient>,
}

impl PodProcessor {
    fn new(pod: Arc<Pod>, client: Arc<AgentClient>) -> Self {
        Self {
            pod,
            processing_tasks: JoinSet::new(),
            client,
        }
    }

    /// Will handle the creation of the pod job, submission to the agent, listening for completion, and extracting the `output_packet` if successful
    async fn start_pod_job_task(
        node_id: String,
        pod: Arc<Pod>,
        packet: HashMap<String, PathSet>,
        client: Arc<AgentClient>,
        session: Arc<zenoh::Session>,
        base_output_key_exp: String,
        namespace: String,
        namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<()> {
        // For now we will just send the input_packet to the success channel
        let node_id_bytes = node_id.as_bytes().to_vec();
        let input_packet_hash = {
            let mut buf = node_id_bytes;
            let mut serializer = Serializer::new(&mut buf);
            serialize_hashmap(&packet, &mut serializer)?;
            hash_buffer(buf)
        };
        let output_dir = URI {
            namespace: namespace.clone(),
            path: PathBuf::from(format!("pod_runs/{node_id}/{input_packet_hash}")),
        };

        let cpu_limit = pod.recommended_cpus;
        let memory_limit = pod.recommended_memory;

        // Create the pod job
        let pod_job = PodJob::new(
            None,
            Arc::clone(&pod),
            packet,
            output_dir,
            cpu_limit,
            memory_limit,
            None,
            namespace_lookup,
        )?;

        // Create listener for pod_job
        let target_key_exp = format!("group/{}/success/pod_job/{}/**", client.group, pod_job.hash);

        // Create the subscriber
        let pod_job_subscriber = session
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
        let responses = client.start_pod_jobs(vec![pod_job.into()]).await;
        let response = responses
            .first()
            .context(selector::InvalidIndex { idx: 0_usize })?;

        match response {
            Response::Ok => (),
            Response::Err(err) => {
                return Err(OrcaError {
                    kind: Kind::PodJobSubmissionFailed {
                        reason: err.clone(),
                        backtrace: Some(snafu::Backtrace::capture()),
                    },
                });
            }
        }

        // Get the pod result from the listener task
        let temp = pod_job_listener_task.await?;

        let pod_result = temp?;

        // Get the output packet for the pod result
        let output_packet = match pod_result.status {
            PodResultStatus::Completed => {
                // Get the output packet
                pod_result.pod_job.get_output_packet(namespace_lookup)?
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

        session
            .put(
                base_output_key_exp.clone() + "/" + SUCCESS_KEY_EXP,
                serde_json::to_string(&NodeOutput::Packet(node_id.clone(), output_packet))?,
            )
            .await
            .context(selector::AgentCommunicationFailure {})?;
        Ok::<(), OrcaError>(())
    }
}

#[async_trait]
impl NodeProcessor for PodProcessor {
    fn process_packet(
        &mut self,
        _sender_node_id: &str,
        node_id: &str,
        packet: &HashMap<String, PathSet>,
        session: Arc<zenoh::Session>,
        base_output_key_exp: &str,
        namespace: &str,
        namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<()> {
        // We need a unique hash for this given input packet process by the node
        // therefore we need to generate a hash that has the pod_id + input_packet
        let pod_clone = Arc::clone(&self.pod);
        let client_clone = Arc::clone(&self.client);
        let node_id_owned = node_id.to_owned();
        let packet_owned = packet.clone();
        let base_output_key_exp_owned = base_output_key_exp.to_owned();
        let namespace_owned = namespace.to_owned();
        let namespace_lookup_owned = namespace_lookup.clone();

        self.processing_tasks.spawn(async move {
            let results = Self::start_pod_job_task(
                node_id_owned.clone(),
                pod_clone,
                packet_owned,
                client_clone,
                Arc::clone(&session),
                base_output_key_exp_owned.clone(),
                namespace_owned.clone(),
                &namespace_lookup_owned,
            )
            .await;

            match results {
                Ok(()) => {}
                Err(err) => {
                    try_to_forward_err_msg(
                        session,
                        err,
                        &base_output_key_exp_owned,
                        &node_id_owned,
                    )
                    .await;
                }
            }
        });
        Ok(())
    }

    async fn mark_parent_as_complete(&mut self, _parent_node_id: &str) -> bool {
        // For pod we only have one parent, thus execute the exit case
        while self.processing_tasks.join_next().await.is_some() {}
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
    processing_tasks: JoinSet<()>,
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
        base_output_key_exp: &str,
        _namespace: &str,
        _namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<()> {
        let mapping = self.mapper.mapping.clone();
        let packet_clone = packet.clone();
        let node_id_clone = node_id.to_owned();
        let output_key_exp_clone = base_output_key_exp.to_owned();

        self.processing_tasks.spawn(async move {
            let result = async {
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
                        format!("{output_key_exp_clone}/{SUCCESS_KEY_EXP}"),
                        &serde_json::to_string(&NodeOutput::Packet(
                            node_id_clone.clone(),
                            output_map,
                        ))?,
                    )
                    .await
                    .context(selector::AgentCommunicationFailure {})?;
                Ok::<(), OrcaError>(())
            }
            .await;

            if let Err(err) = result {
                try_to_forward_err_msg(session, err, &output_key_exp_clone, &node_id_clone).await;
            }
        });
        Ok(())
    }

    async fn mark_parent_as_complete(&mut self, _parent_node_id: &str) -> bool {
        // For mapper we only have one parent, thus execute the exit case
        while (self.processing_tasks.join_next().await).is_some() {
            // The only error that should be forwarded here is the failure to send the output packet
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
#[derive(Debug)]
struct JoinerProcessor {
    /// Cache for all packets received by the node
    input_packet_cache: HashMap<String, Vec<HashMap<String, PathSet>>>,
    completed_parents: Vec<String>,
    processing_tasks: JoinSet<()>,
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
        base_output_key_exp: &str,
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
            let output_key_exp_clone = base_output_key_exp.to_owned();

            self.processing_tasks.spawn(async move {
                // Convert Vec<Vec<HashMap<...>>> to Vec<&Vec<HashMap<...>>> for compute_cartesian_product
                let cartesian_product = Self::compute_cartesian_product(&factors);
                // Post all products to the output channel
                let session_clone = Arc::clone(&session);
                for output_packet in cartesian_product {
                    let result = async {
                        session_clone
                            .put(
                                format!("{output_key_exp_clone}/{SUCCESS_KEY_EXP}"),
                                serde_json::to_string(&NodeOutput::Packet(
                                    node_id_clone.clone(),
                                    output_packet,
                                ))?,
                            )
                            .await
                            .context(selector::AgentCommunicationFailure {})?;
                        Ok::<(), OrcaError>(())
                    }
                    .await;

                    // If the result is an error, we will just send it to the error channel
                    if let Err(err) = result {
                        try_to_forward_err_msg(
                            Arc::clone(&session_clone),
                            err,
                            &output_key_exp_clone,
                            &node_id_clone,
                        )
                        .await;
                    }
                }
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
/// This test 3 cases for the joiner node:
/// The notation is as follows: (parent_id: data_file)
/// 1. Insufficient parents: It should not output anything until all parents has produce a packet (0: [A] 1: [A] 2: []) -> No output
/// 2. Sufficient parents: It should output a single packet with the cartesian product of the parents (0: [A] 1: [A] 2: [A]) -> Output: (0: A, 1: A, 2: A)
/// 3. Additional packet after initial condition is met: It should output a new packet with the cartesian product of the parents (0: [A] 1: [A] 2: [A, B]) -> Output: (0: A, 1: A, 2: B)
/// 4. Add an additional packet where more than 1 packet will be generated: (0: [A, B] 1: [A] 2: [A, B]) -> Output: (0: B, 1: A, 2: A), (0: B, 1: A, 2: B),
async fn joiner() -> Result<()> {
    use std::{thread::sleep, time::Duration};

    let parent_ids = vec!["0".to_owned(), "1".to_owned(), "2".to_owned()];

    let mut joiner_processor = JoinerProcessor::new(parent_ids);
    let session = Arc::new(
        zenoh::open(zenoh::Config::default())
            .await
            .context(selector::AgentCommunicationFailure {})?,
    );

    let base_output_key_exp = "joiner_unit_test".to_owned();

    // Create a buffer and a listener for the output channel
    let success_msg = Arc::new(Mutex::new(Vec::new()));
    let success_sub = session
        .declare_subscriber(format!("{base_output_key_exp}/{SUCCESS_KEY_EXP}"))
        .await
        .context(selector::AgentCommunicationFailure {})?;

    // Create the async test to receive messages from the output channel
    let mut listener_task = JoinSet::new();
    let success_msg_clone = Arc::clone(&success_msg);
    listener_task.spawn(async move {
        while let Ok(msg) = success_sub.recv() {
            success_msg_clone.lock().await.push(msg);
        }
    });

    // Make each parent has 1 packet
    for idx in 0..2 {
        let packet = make_test_packet(format!("key_{idx}"), "data_A.txt".to_owned().into());
        joiner_processor.process_packet(
            &format!("{idx}"),
            &idx.to_string(),
            &packet,
            Arc::clone(&session),
            &base_output_key_exp,
            "",
            &HashMap::new(),
        )?;
    }

    // Confirm that there should be no output yet
    assert!(
        success_msg.lock().await.is_empty(),
        "Should have no messages in the channel",
    );

    // Now we send the missing parent package
    // This will yield one unique combination
    let packet_2_a = make_test_packet("key_2".to_owned(), "data_A.txt".to_owned().into());
    joiner_processor.process_packet(
        "2",
        "2",
        &packet_2_a,
        Arc::clone(&session),
        &base_output_key_exp,
        "",
        &HashMap::new(),
    )?;

    // Wait for the joiner to process and the listener to process the message
    sleep(Duration::from_millis(100));

    // Confirm that the output is sent to the child channel
    assert_eq!(
        success_msg.lock().await.len(),
        1,
        "Should have only one message in the channel",
    );

    let packet_2_b = make_test_packet("key_2".to_owned(), "data_B.txt".to_owned().into());
    joiner_processor.process_packet(
        "2",
        "2",
        &packet_2_b,
        Arc::clone(&session),
        &base_output_key_exp,
        "",
        &HashMap::new(),
    )?;

    // Wait for the joiner to process and the listener to process the message
    sleep(Duration::from_millis(100));

    // The joiner node should send another one
    assert_eq!(
        success_msg.lock().await.len(),
        2,
        "Should have only two messages in the channel",
    );

    let packet_0_b = make_test_packet("key_0".to_owned(), "data_B.txt".to_owned().into());
    joiner_processor.process_packet(
        "0",
        "0",
        &packet_0_b,
        Arc::clone(&session),
        &base_output_key_exp,
        "",
        &HashMap::new(),
    )?;

    // Wait for the joiner to process and the listener to process the message
    sleep(Duration::from_millis(100));

    // Should be a total of 6 messages in the channel
    assert_eq!(
        success_msg.lock().await.len(),
        4,
        "Should have 4 messages in the channel",
    );

    Ok(())
}

/// Helper function to create a test packet with a given key and path
#[cfg(test)]
fn make_test_packet(key: String, path: PathBuf) -> HashMap<String, PathSet> {
    use crate::uniffi::model::packet::{Blob, BlobKind, URI};

    let path_set = PathSet::Unary(Blob {
        kind: BlobKind::File,
        location: URI {
            namespace: "test".to_owned(),
            path,
        },
        checksum: String::new(),
    });

    HashMap::from([(key, path_set)])
}
