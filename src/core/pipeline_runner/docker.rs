use futures_util::stream::FuturesUnordered;
use tokio::{
    sync::broadcast::{self, Receiver, Sender},
    task::JoinSet,
};
use tokio_stream::StreamExt as _;

use super::PipelineRun;
use crate::{
    core::{
        pipeline::{Node, PipelineJob},
        util::get,
    },
    uniffi::{
        error::{Result, selector},
        model::Input,
    },
};
use snafu::OptionExt as _;
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Debug)]
pub(crate) enum Message {
    NodeOutput(String, HashMap<String, Input>), // String is the parent_node_name, while HashMap is output of the parent node
    Stop,                                       // Message to halt all operations
}

struct PipelineRunInfo {
    job_manager_send_handle: Sender<Message>,
    node_tx: HashMap<String, Sender<Message>>,
}

/// Docker based pipeline runner meant to execute on a single machine
#[derive(Default)]
pub struct DockerPipelineRunner {
    pipeline_runs: HashMap<Arc<PipelineRun>, PipelineRunInfo>, // For each pipeline run, we have a join set to track the tasks and wait on them
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
    pub fn start(&mut self, pipeline_job: PipelineJob) -> Result<PipelineRun> {
        // Create a new pipeline run
        let pipeline_run = PipelineRun { pipeline_job };
        let pipeline_run_arc = Arc::new(pipeline_run.clone());

        // Insert into the list of pipeline runs
        self.pipeline_runs.insert(
            pipeline_run_arc.clone(),
            PipelineRunInfo {
                job_manager_send_handle: broadcast::channel::<Message>(1).0,
                node_tx: HashMap::new(),
            },
        );

        // Create the source channel for the pipeline
        // This channel will be used to send inputs to the pipeline
        let (source_tx, _) = broadcast::channel::<Message>(1);

        // Get reference to the pipeline
        let pipeline = &pipeline_run_arc.pipeline_job.pipeline;

        // Get all the leaf nodes and call the create_task_for_node function for each leaf node
        // This will recursively create all the tasks and channels for the pipeline
        for node_key in pipeline.get_leaf_nodes() {
            self.create_task_for_node(node_key, &pipeline_run_arc, &source_tx)?;
        }

        Ok(pipeline_run)
    }

    fn create_task_for_node(
        &mut self,
        node_key: &str,
        pipeline_run: &Arc<PipelineRun>,
        source_tx: &Sender<Message>,
    ) -> Result<Sender<Message>> {
        // Get parents for the node
        let mut parent_channel_rxs = pipeline_run
            .pipeline_job
            .pipeline
            .get_parents_key_for_node(node_key)
            .map(|parent_node_key| {
                // Check if it exists in the pipeline_runs hashmap

                match get(&self.pipeline_runs, pipeline_run)?
                    .node_tx
                    .get(parent_node_key)
                {
                    Some(rx) => Ok(rx.subscribe()),
                    None => {
                        // Missing parent node, thus call create_task for the parent node parent node first
                        Ok(self
                            .create_task_for_node(parent_node_key, pipeline_run, source_tx)?
                            .subscribe())
                    }
                }
            })
            .collect::<Result<Vec<_>>>()?;

        // Check if there is any parents rx
        if parent_channel_rxs.is_empty() {
            // No parents, thus this is root node
            // The parent rx will be the source channel rx
            parent_channel_rxs.push(source_tx.subscribe());
        }

        // Create the channel for this node
        let (tx, _) = broadcast::channel::<Message>(1);

        // Spawn the node_manager for this node
        tokio::spawn(Self::start_node_manager(
            node_key.to_owned(),
            pipeline_run.clone(),
            parent_channel_rxs,
            get(&self.pipeline_runs, pipeline_run)?
                .job_manager_send_handle
                .subscribe(),
            tx.clone(),
        ));

        // Insert it into the the tx into the pipeline_runs hashmap
        self.pipeline_runs
            .get_mut(pipeline_run)
            .context(selector::KeyMissing {
                key: pipeline_run.to_string(),
            })?
            .node_tx
            .insert(node_key.to_owned(), tx.clone());

        // Return tx
        Ok(tx)
    }

    /// For tx: Sender<Message>, we only want to send successfully completed results to the next node
    #[expect(
        clippy::excessive_nesting,
        reason = "This is a complex function that handles multiple tasks and channels"
    )]
    async fn start_node_manager(
        node_key: String,
        pipeline_run: Arc<PipelineRun>,
        parent_channel_rxs: Vec<Receiver<Message>>,
        mut job_manager_channel: Receiver<Message>,
        tx: Sender<Message>,
    ) -> Result<()> {
        // Get the node from the pipeline
        let node = pipeline_run.pipeline_job.pipeline.get_node(&node_key)?;

        // Set up a join_set to track the tasks
        let mut task_join_set = JoinSet::new();

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

        // Listen to the MPSC channel
        while let Some(result) = futures.next().await {
            let rx_result = match result {
                Ok(rx_result) => rx_result,
                Err(e) => {
                    if e.is_panic() {
                        eprintln!("Task panicked: {e}");
                    } else {
                        eprintln!("Error receiving message: {e}");
                    }
                    continue;
                }
            };

            let Ok(msg) = rx_result else {
                eprintln!("Failed to receive message from parent channel");
                continue;
            };

            match msg {
                Message::NodeOutput(_, input_map) => {
                    // Inputs from parents are ready, thus we need to process them if they are already computed and cached
                    // NOTE: Cache is TODO
                    match node {
                        Node::Pod(pod) => {
                            // TODO check if there is already a computed results for this pod

                            // Launch the pod with a copy the tx and manager_tx for success and failure reporting respectively
                            // Compute the pod_job and send it to the orchestrator
                            // NOTE: For now just print the pod name
                            let tx_for_task = tx.clone();
                            let pod_for_task = pod.clone();
                            let node_key_clone = node_key.clone();
                            task_join_set.spawn(async move {
                                // Simulate pod execution
                                println!("Executing pod: {}", pod_for_task.hash);

                                // Simulate successful completion
                                tx_for_task.send(Message::NodeOutput(node_key_clone, input_map))
                            });
                        }
                        Node::Mapper(mapper) => {
                            // For mapper, we just apply it directly
                            let output_map = mapper
                                .mapping
                                .iter()
                                .map(|(input_key, output_key)| {
                                    let input = get(&input_map, input_key)?.clone();
                                    Ok((output_key.to_owned(), input))
                                })
                                .collect::<Result<HashMap<_, _>>>()?;

                            // Send the output via the channel
                            tx.send(Message::NodeOutput(node_key.clone(), output_map))?;
                        }
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
}
