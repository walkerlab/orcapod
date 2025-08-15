use crate::{
    core::{
        crypto::hash_buffer,
        model::{pipeline::PipelineNode, serialize_hashmap},
        operator::{JoinOperator, Operator},
        util::{get, make_key_expr},
    },
    uniffi::{
        error::{
            Kind, OrcaError, Result,
            selector::{self},
        },
        model::{
            packet::{Packet, PathSet, URI},
            pipeline::{Kernel, PipelineJob, PipelineResult},
            pod::{Pod, PodJob, PodResult},
        },
        orchestrator::{
            PodStatus,
            agent::{Agent, AgentClient, Response},
        },
    },
};
use async_trait::async_trait;
use names::{Generator, Name};
use serde::{Deserialize, Serialize};
use serde_yaml::Serializer;
use snafu::{OptionExt as _, ResultExt as _};
use std::{
    collections::{BTreeMap, HashMap},
    fmt::{Display, Formatter, Result as FmtResult},
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::Arc,
};
use tokio::{
    sync::{Mutex, RwLock},
    task::JoinSet,
};

static NODE_OUTPUT_KEY_EXPR: &str = "output";
static FAILURE_KEY_EXP: &str = "failure";

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
    assigned_name: String,
    session: Arc<zenoh::Session>,   // Zenoh session for communication
    agent_client: Arc<AgentClient>, // Zenoh agent client for communication with docker orchestrators
    pipeline_job: Arc<PipelineJob>, // The pipeline job that this run is associated with
    node_tasks: Arc<Mutex<JoinSet<Result<()>>>>, // JoinSet of tasks for each node in the pipeline
    outputs: Arc<RwLock<HashMap<String, Vec<PathSet>>>>, // String is the node key, while hash
    failure_logs: Arc<RwLock<Vec<ProcessingFailure>>>, // Logs of processing failures
    failure_logging_task: Arc<Mutex<JoinSet<Result<()>>>>, // JoinSet of tasks for logging failures
    namespace: String,
    namespace_lookup: HashMap<String, PathBuf>,
}

impl PipelineRun {
    fn make_key_expr(&self, node_id: &str, event: &str) -> String {
        make_key_expr(
            &self.agent_client.group,
            &self.agent_client.host,
            "pipeline_run",
            &BTreeMap::from([
                ("event".to_owned(), event.to_owned()),
                ("node_id".to_owned(), node_id.to_owned()),
                ("pipeline_run_id".to_owned(), self.assigned_name.clone()),
            ]),
        )
    }

    fn make_abort_request_key_exp(&self) -> String {
        make_key_expr(
            &self.agent_client.group,
            &self.agent_client.host,
            "pipeline_run",
            &BTreeMap::from([
                ("event".to_owned(), "abort".to_owned()),
                ("pipeline_run_id".to_owned(), self.assigned_name.clone()),
            ]),
        )
    }

    // Utils functions
    async fn send_packets(&self, node_id: &str, output_packets: &Vec<Packet>) -> Result<()> {
        Ok(self
            .session
            .put(
                self.make_key_expr(node_id, NODE_OUTPUT_KEY_EXPR),
                serde_json::to_string(output_packets)?,
            )
            .await
            .context(selector::AgentCommunicationFailure {})?)
    }

    async fn send_err(&self, node_id: &str, err: OrcaError) {
        let payload = match serde_json::to_string(&err.to_string()) {
            Ok(json) => json,
            Err(serialize_err) => serialize_err.to_string(),
        };

        self.session
            .put(&self.make_key_expr(node_id, FAILURE_KEY_EXP), payload)
            .await
            .context(selector::AgentCommunicationFailure {})
            .unwrap_or_else(|send_err| {
                eprintln!("Failed to send error message for node {node_id}: {send_err}");
            });
    }

    async fn send_abort_request(&self) -> Result<()> {
        Ok(self
            .session
            .put(&self.make_abort_request_key_exp(), vec![])
            .await
            .context(selector::AgentCommunicationFailure {})?)
    }
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
#[derive(Debug, Clone)]
pub struct DockerPipelineRunner {
    agent: Arc<Agent>,
    pipeline_runs: HashMap<String, Arc<PipelineRun>>,
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
    pub fn new(agent: Arc<Agent>) -> Self {
        Self {
            agent,
            pipeline_runs: HashMap::new(),
        }
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
        // Create a new pipeline run
        let pipeline_run = Arc::new(PipelineRun {
            pipeline_job: pipeline_job.into(),
            outputs: Arc::new(RwLock::new(HashMap::new())),
            node_tasks: Arc::new(Mutex::new(JoinSet::new())),
            failure_logs: Arc::new(RwLock::new(Vec::new())),
            failure_logging_task: Arc::new(Mutex::new(JoinSet::new())),
            assigned_name: Generator::with_naming(Name::Plain).next().context(
                selector::MissingInfo {
                    details: "unable to generate a random name",
                },
            )?,
            session: Arc::clone(&self.agent.client.session),
            agent_client: Arc::clone(&self.agent.client),
            namespace: namespace.to_owned(),
            namespace_lookup: namespace_lookup.clone(),
        });

        // Create failure logging task
        pipeline_run
            .failure_logging_task
            .lock()
            .await
            .spawn(Self::failure_capture_task(Arc::clone(&pipeline_run)));

        // Create the processor task for each node
        // The id for the pipeline_run is the pipeline_job hash
        let pipeline_run_id = pipeline_run.pipeline_job.hash.clone();

        let graph = &pipeline_run.pipeline_job.pipeline.graph;

        // Create the subscriber that listen for ready messages
        let subscriber = pipeline_run
            .session
            .declare_subscriber(pipeline_run.make_key_expr("*", "node_ready"))
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
                .lock()
                .await
                .spawn(Self::spawn_node_processing_task(
                    graph[node_idx].clone(),
                    Arc::clone(&pipeline_run),
                    input_nodes.contains(&node.id),
                ));
        }

        // Spawn the task that captures the outputs based on the output_spec
        let mut node_output_spec = HashMap::new();
        // Group the output spec by node
        for (output_key, node_uri) in &pipeline_run.pipeline_job.pipeline.output_spec {
            node_output_spec
                .entry(node_uri.node_id.clone())
                .or_insert_with(HashMap::new)
                .insert(output_key.clone(), node_uri.key.clone());
        }

        for (node_id, key_mapping) in node_output_spec {
            // Spawn the task that captures the outputs
            pipeline_run
                .node_tasks
                .lock()
                .await
                .spawn(Self::create_output_capture_task_for_node(
                    key_mapping,
                    Arc::clone(&pipeline_run),
                    node_id.clone(),
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

        // For each node send all the packets associate with it
        for (node_id, input_packets) in pipeline_run.pipeline_job.get_input_packet_per_node()? {
            // Send the packet to the input node key_exp
            pipeline_run
                .send_packets(&format!("input_node_{node_id}"), &input_packets)
                .await?;

            // Packets are sent, thus we can send the empty vec which signify processing is done
            pipeline_run
                .send_packets(&format!("input_node_{node_id}"), &Vec::new())
                .await?;
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
        while let Some(result) = pipeline_run.node_tasks.lock().await.join_next().await {
            result??;
        }

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
        // Get the pipeline run first then broadcast the abort request signal
        let pipeline_run =
            self.pipeline_runs
                .get_mut(pipeline_run_id)
                .context(selector::KeyMissing {
                    key: pipeline_run_id.to_owned(),
                })?;

        // Send the abort request signal
        pipeline_run.send_abort_request().await?;

        while pipeline_run
            .node_tasks
            .lock()
            .await
            .join_next()
            .await
            .is_some()
        {}
        Ok(())
    }

    /// This will capture the outputs of the given nodes and store it in the `outputs` map
    async fn create_output_capture_task_for_node(
        //<Key to pull from the node, Key that will be mapped to in the outputs>
        key_mapping: HashMap<String, String>,
        pipeline_run: Arc<PipelineRun>,
        node_id: String,
    ) -> Result<()> {
        // Determine which keys we are interested in for the given node_id

        // Create a zenoh session
        let subscriber = pipeline_run
            .session
            .declare_subscriber(pipeline_run.make_key_expr(&node_id, NODE_OUTPUT_KEY_EXPR))
            .await
            .context(selector::AgentCommunicationFailure {})?;

        while let Ok(payload) = subscriber.recv_async().await {
            println!(
                "Received output from node {}: {}",
                node_id,
                String::from_utf8_lossy(&payload.payload().to_bytes())
            );
            // Extract the message from the payload
            let packets: Vec<Packet> = serde_json::from_slice(&payload.payload().to_bytes())?;

            if packets.is_empty() {
                // Output node exited, thus we can exit the capture task too
                break;
            }
            let mut outputs_lock = pipeline_run.outputs.write().await;

            for packet in packets {
                for (output_key, node_key) in &key_mapping {
                    outputs_lock
                        .entry(output_key.to_owned())
                        .or_default()
                        .push(get(&packet, node_key)?.clone());
                }
            }
        }
        Ok(())
    }

    async fn failure_capture_task(pipeline_run: Arc<PipelineRun>) -> Result<()> {
        let sub = pipeline_run
            .session
            .declare_subscriber(pipeline_run.make_key_expr("*", FAILURE_KEY_EXP))
            .await
            .context(selector::AgentCommunicationFailure {})?;

        // Listen to any failure messages and write it the logs
        while let Ok(payload) = sub.recv_async().await {
            // Extract the message from the payload
            let process_failure: ProcessingFailure =
                serde_json::from_slice(&payload.payload().to_bytes())?;
            // Store the failure message in the logs
            pipeline_run
                .failure_logs
                .write()
                .await
                .push(process_failure.clone());
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
        pipeline_run: Arc<PipelineRun>,
        is_input_node: bool,
    ) -> Result<()> {
        // Get the node parents
        let parent_nodes = pipeline_run
            .pipeline_job
            .pipeline
            .get_node_parents(&node)
            .collect::<Vec<_>>();

        // Create the correct processor for the node based on the kernel type
        let node_processor: Arc<Mutex<Box<dyn NodeProcessor>>> =
            Arc::new(Mutex::new(match &node.kernel {
                Kernel::Pod { pod } => Box::new(PodProcessor::new(
                    Arc::clone(&pipeline_run),
                    node.id.clone(),
                    Arc::clone(pod),
                )),
                Kernel::MapOperator { mapper } => Box::new(OperatorProcessor::new(
                    Arc::clone(&pipeline_run),
                    node.id.clone(),
                    Arc::clone(mapper),
                    parent_nodes.len(),
                )),
                Kernel::JoinOperator => Box::new(OperatorProcessor::new(
                    Arc::clone(&pipeline_run),
                    node.id.clone(),
                    JoinOperator::new(parent_nodes.len()).into(),
                    parent_nodes.len(),
                )),
            }));

        // Create a join set to spawn and handle incoming messages tasks
        let mut listener_tasks = JoinSet::new();

        // Create a list of node_ids that this node should listen to
        let mut nodes_to_sub_to = parent_nodes
            .iter()
            .map(|parent_node| parent_node.id.clone())
            .collect::<Vec<_>>();

        if is_input_node {
            // If the node is an input node, we need to add the input node key expression
            nodes_to_sub_to.push(format!("input_node_{}", node.id));
        }

        // For each node in nodes_to_subscribe_to, call the event handler func
        for node_to_sub in &nodes_to_sub_to {
            listener_tasks.spawn(Self::event_handler(
                Arc::clone(&pipeline_run),
                node.id.clone(),
                node_to_sub.to_owned(),
                Arc::clone(&node_processor),
            ));
        }

        // Create the listener task for the stop request
        let abort_request_handler_task = tokio::spawn(Self::abort_request_event_handler(
            node_processor,
            Arc::clone(&pipeline_run),
        ));

        // Wait for all tasks to be spawned and reply with ready message
        // This is to ensure that the pipeline run knows when all tasks are ready to receive inputs
        let mut num_of_ready_event_handler: usize = 0;
        // Build the subscriber
        let status_subscriber = pipeline_run
            .session
            .declare_subscriber(pipeline_run.make_key_expr(&node.id, "event_handler_ready"))
            .await
            .context(selector::AgentCommunicationFailure {})?;

        while status_subscriber.recv_async().await.is_ok() {
            num_of_ready_event_handler += 1;
            if num_of_ready_event_handler == nodes_to_sub_to.len() {
                // +1 for the stop request task
                break; // All tasks are ready, we can start sending inputs
            }
        }

        // Send a ready message so the pipeline knows when to start sending inputs
        pipeline_run
            .session
            .put(pipeline_run.make_key_expr(&node.id, "node_ready"), vec![])
            .await
            .context(selector::AgentCommunicationFailure {})?;

        // Wait for all task to complete
        while let Some(result) = listener_tasks.join_next().await {
            match result {
                Ok(Ok(())) => {} // Task completed successfully
                Ok(Err(err)) => {
                    pipeline_run.send_err(&node.id, err).await;
                }
                Err(err) => {
                    pipeline_run.send_err(&node.id, OrcaError::from(err)).await;
                }
            }
        }

        // Abort the stop listener task since we don't need it anymore
        abort_request_handler_task.abort();

        Ok(())
    }

    /// This is the actual handler for incoming messages for the node
    async fn event_handler(
        pipeline_run: Arc<PipelineRun>,
        node_id: String,
        node_to_sub_to: String,
        processor: Arc<Mutex<Box<dyn NodeProcessor>>>,
    ) -> Result<()> {
        // Create the subscriber
        let subscriber = pipeline_run
            .session
            .declare_subscriber(pipeline_run.make_key_expr(&node_to_sub_to, NODE_OUTPUT_KEY_EXPR))
            .await
            .context(selector::AgentCommunicationFailure {})?;

        // Send out ready signal
        pipeline_run
            .session
            .put(
                pipeline_run.make_key_expr(&node_id, "event_handler_ready"),
                vec![],
            )
            .await
            .context(selector::AgentCommunicationFailure {})?;

        // Listen to the key
        loop {
            let sample = subscriber
                .recv_async()
                .await
                .context(selector::AgentCommunicationFailure)?;

            // Extract out the packets
            let packets: Vec<Packet> = serde_json::from_slice(&sample.payload().to_bytes())?;

            // Check if the packets are empty, if so that means the node is finished processing
            if packets.is_empty() {
                processor
                    .lock()
                    .await
                    .mark_parent_as_complete(&node_to_sub_to)
                    .await;
                break;
            }

            // For each packet, we need to process it
            for packet in packets {
                processor
                    .lock()
                    .await
                    .process_incoming_packet(&node_to_sub_to, &packet)
                    .await;
            }
        }
        Ok::<(), OrcaError>(())
    }

    /// This task will listen for stop requests on the given key expression
    async fn abort_request_event_handler(
        node_processor: Arc<Mutex<Box<dyn NodeProcessor>>>,
        pipeline_run: Arc<PipelineRun>,
    ) -> Result<()> {
        let subscriber = pipeline_run
            .session
            .declare_subscriber(pipeline_run.make_abort_request_key_exp())
            .await
            .context(selector::AgentCommunicationFailure {})?;
        while subscriber.recv_async().await.is_ok() {
            // Received a request to stop, therefore we need to tell the node_processor to shutdown
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
    async fn process_incoming_packet(&mut self, sender_node_id: &str, incoming_packet: &Packet);

    /// Notifies the processor that the parent node has completed processing
    /// If it is the last parent to complete, it will wait for all processing task to finish
    /// Then send a completion signal
    async fn mark_parent_as_complete(&mut self, parent_node_id: &str);

    fn stop(&mut self);
}

/// Processor for Pods
/// Currently missing implementation to call agents for actual pod processing
struct PodProcessor {
    pipeline_run: Arc<PipelineRun>,
    node_id: String,
    pod: Arc<Pod>,
    processing_tasks: JoinSet<()>,
}

impl PodProcessor {
    fn new(pipeline_run: Arc<PipelineRun>, node_id: String, pod: Arc<Pod>) -> Self {
        Self {
            pipeline_run,
            node_id,
            pod,
            processing_tasks: JoinSet::new(),
        }
    }
}

impl PodProcessor {
    /// Will handle the creation of the pod job, submission to the agent, listening for completion, and extracting the `output_packet` if successful
    async fn process_packet(
        pipeline_run: Arc<PipelineRun>,
        node_id: String,
        pod: Arc<Pod>,
        incoming_packet: HashMap<String, PathSet>,
    ) -> Result<Packet> {
        // Hash the input_packet to create a unique identifier for the pod job
        let input_packet_hash = {
            let mut buf = Vec::new();
            let mut serializer = Serializer::new(&mut buf);
            serialize_hashmap(&incoming_packet, &mut serializer)?;
            hash_buffer(buf)
        };

        // Create the pod job
        let pod_job = PodJob::new(
            None,
            Arc::clone(&pod),
            incoming_packet,
            URI {
                namespace: pipeline_run.namespace.clone(),
                path: format!(
                    "pipeline_outputs/{}/{node_id}/{input_packet_hash}",
                    pipeline_run.assigned_name
                )
                .into(),
            },
            pod.recommended_cpus,
            pod.recommended_memory,
            None,
            &pipeline_run.namespace_lookup,
        )?;

        // Create listener for pod_job
        // Create the subscriber
        let pod_job_subscriber = pipeline_run
            .session
            .declare_subscriber(pipeline_run.agent_client.make_key_expr(
                true,
                "pod_job",
                BTreeMap::from([("hash", pod_job.hash.clone()), ("event", "*".to_owned())]),
            ))
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
        let responses = pipeline_run
            .agent_client
            .start_pod_jobs(vec![pod_job.clone().into()])
            .await;
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
        Ok(match pod_result.status {
            PodStatus::Completed => {
                // Get the output packet
                pod_result.output_packet
            }
            PodStatus::Failed(exit_code) => {
                // Processing failed, thus return the error
                return Err(OrcaError {
                    kind: Kind::PodJobProcessingError {
                        hash: pod_result.pod_job.hash.clone(),
                        reason: format!("Pod processing failed with exit code {exit_code}"),
                        backtrace: Some(snafu::Backtrace::capture()),
                    },
                });
            }
            PodStatus::Running | PodStatus::Unset => {
                // This should not happen, but if it does, we will return an error
                return Err(OrcaError {
                    kind: Kind::PodJobProcessingError {
                        hash: pod_result.pod_job.hash.clone(),
                        reason: "Pod result status is running or unset".to_owned(),
                        backtrace: Some(snafu::Backtrace::capture()),
                    },
                });
            }
        })
    }
}

#[async_trait]
impl NodeProcessor for PodProcessor {
    async fn process_incoming_packet(
        &mut self,
        _sender_node_id: &str,
        incoming_packet: &HashMap<String, PathSet>,
    ) {
        // Clone all necessary fields from self to move into the async block
        let pipeline_run = Arc::clone(&self.pipeline_run);
        let node_id = self.node_id.clone();
        let pod = Arc::clone(&self.pod);

        let incoming_packet_inner = incoming_packet.clone();

        self.processing_tasks.spawn(async move {
            let result = match Self::process_packet(
                Arc::clone(&pipeline_run),
                node_id.clone(),
                Arc::clone(&pod),
                incoming_packet_inner.clone(),
            )
            .await
            {
                Ok(output_packet) => {
                    match pipeline_run
                        .send_packets(&node_id, &vec![output_packet])
                        .await
                    {
                        Ok(()) => Ok(()),
                        Err(err) => Err(err),
                    }
                }
                Err(err) => Err(err),
            };

            match result {
                Ok(()) => {
                    // Successfully processed the packet, nothing to do
                }
                Err(err) => {
                    pipeline_run.send_err(&node_id, err).await;
                }
            }
        });
    }

    async fn mark_parent_as_complete(&mut self, _parent_node_id: &str) {
        // For pod we only have one parent, thus execute the exit case
        while self.processing_tasks.join_next().await.is_some() {}
        // Send out completion signal
        match self
            .pipeline_run
            .send_packets(&self.node_id, &Vec::new())
            .await
        {
            Ok(()) => {}
            Err(err) => {
                self.pipeline_run.send_err(&self.node_id, err).await;
            }
        }
    }

    fn stop(&mut self) {
        self.processing_tasks.abort_all();
    }
}

struct OperatorProcessor<T: Operator + Send + Sync> {
    pipeline_run: Arc<PipelineRun>,
    node_id: String,
    operator: Arc<T>,
    num_of_parents: usize,
    num_of_completed_parents: usize,
    processing_tasks: JoinSet<()>,
}

impl<T: Operator + Send + Sync + 'static> OperatorProcessor<T> {
    /// Create a new operator processor
    pub fn new(
        pipeline_run: Arc<PipelineRun>,
        node_id: String,
        operator: Arc<T>,
        num_of_parents: usize,
    ) -> Self {
        Self {
            pipeline_run,
            node_id,
            operator,
            num_of_parents,
            num_of_completed_parents: 0,
            processing_tasks: JoinSet::new(),
        }
    }
}

#[async_trait]
impl<T: Operator + Send + Sync + 'static> NodeProcessor for OperatorProcessor<T> {
    async fn process_incoming_packet(
        &mut self,
        sender_node_id: &str,
        incoming_packet: &HashMap<String, PathSet>,
    ) {
        // Clone all necessary fields from self to move into the async block
        let operator = Arc::clone(&self.operator);
        let pipeline_run = Arc::clone(&self.pipeline_run);
        let node_id = self.node_id.clone();

        let sender_node_id_inner = sender_node_id.to_owned();
        let incoming_packet_inner = incoming_packet.clone();

        self.processing_tasks.spawn(async move {
            let processing_result = operator
                .process_packet(sender_node_id_inner, incoming_packet_inner)
                .await;

            match processing_result {
                Ok(output_packets) => {
                    if !output_packets.is_empty() {
                        // Send out all the packets
                        match pipeline_run.send_packets(&node_id, &output_packets).await {
                            Ok(()) => {}
                            Err(err) => {
                                pipeline_run.send_err(&node_id, err).await;
                            }
                        }
                    }
                }
                Err(err) => {
                    pipeline_run.send_err(&node_id, err).await;
                }
            }
        });
    }

    async fn mark_parent_as_complete(&mut self, _parent_node_id: &str) {
        // Figure out if this is the last parent or not
        self.num_of_completed_parents += 1;

        if self.num_of_completed_parents == self.num_of_parents {
            // All parents are complete, thus we need to wait on all processing tasks then exit
            while (self.processing_tasks.join_next().await).is_some() {
                // Wait for all tasks to complete
            }
            // Send out completion signal which is same as success but it is an empty vec of packets
            match self
                .pipeline_run
                .send_packets(&self.node_id, &Vec::new())
                .await
            {
                Ok(()) => {}
                Err(err) => {
                    self.pipeline_run.send_err(&self.node_id, err).await;
                }
            }
        }
    }

    fn stop(&mut self) {
        todo!()
    }
}
