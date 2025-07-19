use crate::{
    core::{crypto::hash_buffer, model::serialize_hashmap, util::get},
    uniffi::{
        error::{OrcaError, Result, selector},
        model::{PathSet, Pod, PodJob, URI},
        pipeline::{Kernel, Mapper, Node, PipelineJob, PipelineResult},
    },
};
use bincode::{Decode, Encode, config, serde::encode_to_vec};
use futures_util::future::try_join_all;
use itertools::Itertools as _;
use serde::{Deserialize, Serialize};
use serde_yaml::Serializer;
use snafu::{OptionExt as _, ResultExt};
use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::Arc,
};
use tokio::{
    sync::{RwLock, mpsc},
    task::{JoinSet, spawn_blocking},
};

static SUCCESS_KEY_EXP: &str = "/success";
static FAILURE_KEY_EXP: &str = "/failure";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) enum Message {
    /// String is the `parent_node_id`, while `HashMap` is output of the parent node
    NodeOutput(String, HashMap<String, PathSet>),
    NodeProcessingFailure(String, String), // String is the `node_id` that has failed processing
    /// String is the `node_id` that has completed processing
    NodeProcessingComplete(String),
    Stop, // Message to halt all operations
}

#[expect(
    clippy::type_complexity,
    reason = "too complex, but necessary for async handling"
)]
#[derive(Debug, Clone)]
pub struct PipelineRun {
    /// PipelineJob that this run is associated with
    pub pipeline_job: PipelineJob, // The pipeline job that this run is associated with
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
    pipeline_runs: HashSet<Arc<PipelineRun>>,
}

/**
 * This is an implementation of a pipeline runner that uses Zenoh to communicate between the tasks
 * The runtime is tokio
 *
 * These are the key expressions of the components of the pipeline:
 * - Input Node: pipeline_job_hash/input_node/outputs (This is where the pipeline_job packets get fed to)
 * - Nodes: pipeline_job_hash/node_id/outputs/(success|failure) (This is where the node outputs are sent to)
*/
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
    ) -> Result<&PipelineRun> {
        // Create a new pipeline run
        let pipeline_run = Arc::new(PipelineRun {
            pipeline_job,
            outputs: Arc::new(RwLock::new(HashMap::new())),
        });

        // Get reference to the pipeline
        let pipeline = &pipeline_run.pipeline_job.pipeline;

        // Create a task for each node

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

        // Insert into the list of pipeline runs
        self.pipeline_runs.insert(pipeline_run);

        Ok(self
            .pipeline_runs
            .get(&pipeline_run_arc)
            .context(selector::KeyMissing {
                key: pipeline_run.to_string(),
            })?)
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

    /**
     * Act as the processor of the node by:
     * 1. Creating a metadata struct for the node to be passed to the appropriate processor
     * 2. Get the kernel for the node and build the correct processor for this node
     * 3. Start the processor and wait till it completes
     * 4. Send a message that the node processing is complete
     *
     * # Errors
     * Will error out if the kernel for the node is not found or if the
     */
    async fn start_node_task(
        kernel: Kernel,
        output_key_expression: String,
        namespace_path: PathBuf,
    ) -> Result<()> {
        // Get the kernel for this node and build the correct processor
        match kernel {
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

/// Unify the interface for node processors and provide a common way to handle processing of incoming messages
/// This trait defines the methods that all node processors should implement
///
/// Main purpose was to reduce the amount of code duplication between different node processors
/// As a result, each processor only needs to worry about writing their own function to process the msg.
trait NodeProcessor {
    async fn process_packet(
        &mut self,
        sender_node_id: &str,
        node_id: &str,
        packet: &HashMap<String, PathSet>,
        session: Arc<zenoh::Session>,
        output_key_exp: &str,
        namespace: &str,
        namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<()>;

    async fn wait_for_node_task_completion(&mut self) -> Result<()>;

    fn stop(&mut self) -> Result<()>;
}

/// Processor for Pods
/// Currently missing implementation to call agents for actual pod processing
struct PodProcessor {
    session: zenoh::Session,
    pod: Arc<Pod>,
    processing_tasks: JoinSet<Result<(), OrcaError>>,
}

impl PodProcessor {
    fn new(pod: Arc<Pod>) -> Self {
        Self {
            session: zenoh::Session::default(),
            pod,
            processing_tasks: JoinSet::new(),
        }
    }

    /// Actual logic of processing a packet using the pod
    /// At the moment it does a simulation of pod execution
    async fn process_packet(
        _sender_node_id: &str,
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
    async fn process_packet(
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
            &namespace_lookup,
        )?;

        // Simulate pod execution by just printing out pod_job_hash and pod hash
        // This will be replaced by sending the pod_job to the orchestrator via the agent

        // Build the output_packet, in reality, this will be extracted from the pod_result
        #[expect(
            clippy::unwrap_used,
            reason = "Hard code for now, will be replaced by agent"
        )]
        let output_packet = self
            .pod
            .output_spec
            .keys()
            .map(|output_key| (output_key.clone(), packet.values().next().cloned().unwrap()))
            .collect::<HashMap<_, _>>();

        let node_id_clone = node_id.to_owned();
        let output_key_exp_clone = output_key_exp.to_owned();
        self.processing_tasks.spawn(async move {
            println!(
                "Simulating Executing pod job: {} with pod hash: {}",
                pod_job.hash, pod_job.pod.hash
            );

            // For now we will just send the input_packet to the success channel
            session
                .put(
                    output_key_exp_clone + SUCCESS_KEY_EXP,
                    bincode::serde::encode_to_vec(
                        &Message::NodeOutput(node_id_clone, output_packet),
                        bincode::config::standard(),
                    )?,
                )
                .await
                .context(selector::AgentCommunicationFailure {})?;

            Ok(())
        });

        Ok(())
    }

    async fn wait_for_node_task_completion(&mut self) -> Result<()> {
        while self.processing_tasks.join_next().await.is_some() {}
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        self.processing_tasks.abort_all();
        Ok(())
    }
}

/// Processor for Mapper nodes
/// This processor renames the `input_keys` from the input packet to the `output_keys` defined by the map
struct MapperProcessor {
    mapper: Arc<Mapper>,
}

impl MapperProcessor {
    const fn new(mapper: Arc<Mapper>) -> Self {
        Self { mapper }
    }
}

impl NodeProcessor for MapperProcessor {
    async fn process_packet(
        &mut self,
        _sender_node_id: &str,
        node_id: &str,
        packet: &HashMap<String, PathSet>,
        session: Arc<zenoh::Session>,
        output_key_exp: &str,
        _namespace: &str,
        _namespace_lookup: &HashMap<String, PathBuf>,
    ) -> Result<()> {
        // Apply the mapping to the input packet
        let output_map = self
            .mapper
            .mapping
            .iter()
            .map(|(input_key, output_key)| {
                let input = get(&packet, input_key)?.clone();
                Ok((output_key.to_owned(), input))
            })
            .collect::<Result<HashMap<_, _>>>()?;

        // Send the packet outwards
        session
            .put(
                output_key_exp,
                bincode::serde::encode_to_vec(
                    &Message::NodeOutput(node_id.to_owned(), output_map),
                    bincode::config::standard(),
                )?,
            )
            .await
            .context(selector::AgentCommunicationFailure {})?;

        Ok(())
    }

    async fn wait_for_node_task_completion(&mut self) -> Result<()> {
        // All mappers tasks are synchronous, so we don't need to wait for anything
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        // Mappers do not have any state to stop, so we can just return Ok
        Ok(())
    }
}

/// Processor for Joiner nodes
/// This processor combines packets from multiple parent nodes into a single output packet
/// It uses a cartesian product to combine packets from different parents
struct JoinerProcessor {
    /// Cache for all packets received by the node
    input_packet_cache: HashMap<String, Vec<HashMap<String, PathSet>>>,
    completed_parents: Vec<String>,
    initial_computation_completed: bool,
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
            initial_computation_completed: false,
            processing_tasks: JoinSet::new(),
        }
    }

    fn compute_cartesian_product(
        factors: &Vec<Vec<HashMap<String, PathSet>>>,
    ) -> Vec<HashMap<String, PathSet>> {
        factors
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
            .collect::<Vec<_>>()
    }
}

impl NodeProcessor for JoinerProcessor {
    async fn process_packet(
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
                .map(|id| get(&self.input_packet_cache, id).map(|v| v.clone()))
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
                                output_key_exp_clone.clone() + SUCCESS_KEY_EXP,
                                encode_to_vec(
                                    &Message::NodeOutput(node_id_clone.clone(), output_packet),
                                    config::standard(),
                                )?,
                            )
                            .await
                            .context(selector::AgentCommunicationFailure {})?;
                        Ok::<(), OrcaError>(())
                    };

                    // If the result is an error, we will just send it to the error channel
                    if let Err(err) = result {
                        session
                            .put(
                                output_key_exp_clone.clone() + FAILURE_KEY_EXP,
                                encode_to_vec(
                                    &Message::NodeProcessingFailure(
                                        node_id_clone.clone(),
                                        err.to_string(),
                                    ),
                                    config::standard(),
                                )?,
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

    async fn wait_for_node_task_completion(&mut self) -> Result<()> {
        // We must wait for all joiner processing task to complete
        while self.processing_tasks.join_next().await.is_some() {}
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        // We want to abort any computation
        self.processing_tasks.abort_all();
        Ok(())
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
