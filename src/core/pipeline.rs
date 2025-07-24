use crate::{
    core::{
        operator::{JoinOperator, MapOperator, Operator},
        orchestrator::agent::RE_AGENT_KEY_EXPR,
    },
    uniffi::{
        error::{OrcaError, Result, selector},
        model::{
            Kernel, Packet, PathSet, PipelineJob, PipelineResult, Pod, PodJob, PodResult, URI,
        },
        orchestrator::{PodStatus, agent::AgentClient},
        pipeline::PipelineStatus,
    },
};
use chrono::Utc;
use futures_util::{
    TryFutureExt as _,
    future::{FutureExt as _, join_all},
};
use hex;
use itertools::Itertools as _;
use petgraph::{Direction, visit::IntoNodeReferences as _};
use rand::{self, RngCore as _};
use serde::{Deserialize, Serialize};
use snafu::{OptionExt as _, ResultExt as _};
use std::{
    cmp::Ordering,
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    sync::mpsc::{self, error::SendError},
    task::JoinSet,
    time::sleep as async_sleep,
};
use tokio_util::task::TaskTracker;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PipelineNode {
    pub name: String,
    pub kernel: Kernel,
}

impl Ord for PipelineNode {
    fn cmp(&self, other: &Self) -> Ordering {
        self.name.cmp(&other.name)
    }
}

impl PartialOrd for PipelineNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for PipelineNode {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for PipelineNode {}

#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub state: NodeState,
    pub completed_packets: u32,
}

// + err state
#[derive(Debug, Clone)]
pub enum NodeState {
    Idle,
    Active,
    Completed,
    Cancelled,
    Failed(String),
}

#[derive(Deserialize, Serialize)]
pub enum Payload<SM, EM> {
    /// upstream passed data, create a new pod job request, SM can be Vec<Packet> (mpsc) or a single Packet (zenoh)
    Stream(SM),
    /// upstream finished, can finish once idle. EM can be parent name (mpsc) or () (zenoh)
    End(EM),
    /// failed upstream, terminate immediately
    Cancelled,
    Failed(String),
}

enum ActualKernel {
    Pod(Arc<Pod>),
    JoinOperator(Arc<Mutex<JoinOperator>>),
    MapOperator(Arc<Mutex<MapOperator>>),
}

// this is actually more like prepare the pipeline
#[expect(
    unused_variables,
    clippy::too_many_lines,
    clippy::cast_sign_loss,
    clippy::excessive_nesting,
    reason = "debug"
)]
pub async fn process_pipeline_job(
    agent_client: Arc<AgentClient>,
    status_topic: String, // updates to state of pipeline run
    pod_job_request_topic: &str,
    pod_job_success_topic: &str,
    pod_job_failure_topic: &str,
    pipeline_job: PipelineJob,
    created: u64,
    namespace_lookup: HashMap<String, PathBuf>,
) -> Result<PipelineResult> {
    let pipeline_job_input_packet = &pipeline_job.input_packet;
    let input_node_config = pipeline_job
        .pipeline
        .input_spec
        .iter()
        .flat_map(|(key, specs)| {
            specs.iter().flat_map(move |spec| {
                pipeline_job_input_packet[key]
                    .iter()
                    .map(move |path_set| (&spec.node, &spec.key, path_set))
            })
        })
        .fold(
            HashMap::<&String, HashMap<&String, Vec<&PathSet>>>::new(),
            |mut inputs, (node, key, path_set)| {
                if let Some(packet) = inputs.get_mut(node) {
                    if let Some(path_sets) = packet.get_mut(key) {
                        path_sets.push(path_set);
                    } else {
                        packet.insert(key, vec![path_set]);
                    }
                } else {
                    inputs.insert(node, HashMap::from([(key, vec![path_set])]));
                }
                inputs
            },
        );
    // todo: optimize to a single collect
    let input_node_packets = input_node_config
        .clone()
        .into_iter()
        .map(|(node, config)| {
            (
                node,
                config
                    .into_iter()
                    .map(|(key, path_sets)| {
                        path_sets
                            .into_iter()
                            .map(move |path_set| (key.to_owned(), path_set.to_owned()))
                    })
                    .multi_cartesian_product()
                    .map(|path_set_combinations| {
                        path_set_combinations.into_iter().collect::<HashMap<_, _>>()
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<HashMap<_, _>>();

    let mut node_states_handle = JoinSet::new();
    node_states_handle.spawn({
        let pipeline = Arc::clone(&pipeline_job.pipeline);
        let input_nodes = input_node_config.into_keys().cloned().collect::<Vec<_>>();
        let pipeline_job_output_dir = pipeline_job.output_dir.clone();
        let pipeline_job_hash = pipeline_job.hash.clone();
        let inner_agent_client = Arc::clone(&agent_client);
        async move {
            join_all(
                pipeline
                    .graph
                    .node_references()
                    .map(|(child_index, child)| {
                        let inner_inner_agent_client = Arc::clone(&inner_agent_client);
                        let inner_pipeline_job_hash = pipeline_job_hash.clone();
                        let inner_pipeline = Arc::clone(&pipeline);
                        let inner_pipeline_job_output_dir = pipeline_job_output_dir.clone();
                        let inner_namespace_lookup = namespace_lookup.clone();
                        let inner_input_nodes = input_nodes.clone();
                        async move {
                            let parents = if inner_input_nodes.contains(&child.name) {
                                vec![]
                            } else {
                                inner_pipeline
                                    .graph
                                    .neighbors_directed(child_index, Direction::Incoming)
                                    .map(|parent_index| {
                                        inner_pipeline.graph[parent_index].name.clone()
                                    })
                                    .collect::<Vec<String>>()
                            };
                            let kernel = match &child.kernel {
                                Kernel::Pod { r#ref } => ActualKernel::Pod(Arc::clone(r#ref)),
                                Kernel::JoinOperator => ActualKernel::JoinOperator(
                                    Mutex::new(JoinOperator::new(parents.len())).into(),
                                ),
                                Kernel::MapOperator { map } => ActualKernel::MapOperator(
                                    Mutex::new(MapOperator::new(map)).into(),
                                ),
                            };

                            start_subscriptions(
                                Arc::clone(&inner_inner_agent_client),
                                inner_pipeline_job_output_dir,
                                format!("status/pipeline_job/{}", &inner_pipeline_job_hash,),
                                parents,
                                child.name.clone(),
                                kernel.into(),
                                &inner_namespace_lookup,
                            )
                            .await
                        }
                    }),
            )
            .await
            .into_iter()
            .map(|result| {
                let (_, node_state) = result?;
                Ok(node_state)
            })
            .collect::<Result<Vec<_>>>()
        }
    });

    async_sleep(Duration::from_millis(500)).await; // allow all subscribers to start

    for (node, packets) in input_node_packets {
        let input_topic = format!("status/pipeline_job/{}/input/{}", &pipeline_job.hash, node);
        for packet in packets {
            agent_client
                .publish(&input_topic, &Payload::Stream::<_, ()>(packet))
                .await?; // this could be parallelized later with minimal gain
        }
        agent_client
            .publish(&input_topic, &Payload::End::<Packet, _>(()))
            .await?;
    }

    let node_states = node_states_handle
        .join_next()
        .await
        .context(selector::NoRemainingServices {})???;

    let terminated = Utc::now().timestamp() as u64;
    if node_states
        .iter()
        .all(|node_state| matches!(node_state, NodeState::Completed))
    {
        return Ok(PipelineResult {
            pipeline_job: pipeline_job.into(),
            created,
            terminated,
            status: PipelineStatus::Completed,
        });
    }
    Ok(PipelineResult {
        pipeline_job: pipeline_job.into(),
        created,
        terminated,
        status: PipelineStatus::Failed,
    })

    // for (child_index, child) in pipeline_job.pipeline.graph.node_references() {
    //     if input_nodes.contains(&child) {
    //         continue;
    //     }
    //     let parents = pipeline_job
    //         .pipeline
    //         .graph
    //         .neighbors_directed(child_index, Direction::Incoming)
    //         .map(|parent_index| {
    //             (
    //                 parent_index,
    //                 pipeline_job.pipeline.graph[parent_index].clone(),
    //             )
    //         });
    //     let kernel = match get(&pipeline_job.pipeline.metadata, child)? {
    //         Kernel::Pod { r#ref } => ActualKernel::Pod(Arc::clone(r#ref)),
    //         Kernel::JoinOperator => {
    //             ActualKernel::JoinOperator(Mutex::new(JoinOperator::new(parents.count())).into())
    //         }
    //         Kernel::MapOperator { map } => {
    //             ActualKernel::MapOperator(Mutex::new(MapOperator::new(map)).into())
    //         }
    //     };
    // }
}

// node events
// output packet
// end of packets

// pipeline events
// request
// failure

// const fn start_stuff() {
// need to listen to input packets
// on packet prepare podjob request and send
// on podjob success, publish packet to output topic
// on podjob failure,
// on last packet, forward last packet when done

// received_end = false
// mut packet log and we remove as they complete from pod_job, if fail, return early with status
// }

// fn start_node(agent_client: Arc<AgentClient>, parents_topics: Vec<String>) {}

// process a single packet -> pod job -> and await for it to become a pod result
// fn

#[expect(
    clippy::excessive_nesting,
    clippy::let_underscore_must_use,
    // clippy::unwrap_used,
    clippy::too_many_lines,
    reason = "debug."
)]
async fn start_subscriptions(
    agent_client: Arc<AgentClient>,
    pipeline_job_output_dir: URI,
    status_topic: String,
    parents: Vec<String>,
    child: String,
    child_kernel: Arc<ActualKernel>,
    namespace_lookup: &HashMap<String, PathBuf>,
) -> Result<(String, NodeState)> {
    let (response_tx, mut response_rx) = mpsc::channel(100);
    let inner_response_tx = response_tx.clone();
    let send_result = |response| async move {
        let _: Result<_, SendError<_>> = inner_response_tx.send(response).await;
        Ok::<_, OrcaError>(())
    };

    let child_output_dir = URI {
        namespace: pipeline_job_output_dir.namespace,
        path: pipeline_job_output_dir.path.join(&child),
    };
    let child_active_packets = TaskTracker::new();

    let mut services = JoinSet::new();
    if parents.is_empty() {
        services.spawn({
            let inner_send_result = send_result.clone();
            let inner_namespace_lookup = namespace_lookup.clone();
            let inner_status_topic = status_topic.clone();
            let inner_child = child.clone();
            let inner_child_output_dir = child_output_dir.clone();
            let inner_child_kernel = Arc::clone(&child_kernel);
            let inner_agent_client = Arc::clone(&agent_client);
            let inner_child_active_packets = child_active_packets.clone();
            async move {
                start_packet_processor(
                    &inner_agent_client,
                    &inner_status_topic,
                    "input",
                    &inner_child,
                    &inner_child,
                    &inner_child_output_dir,
                    &inner_child_kernel,
                    inner_child_active_packets,
                    &inner_namespace_lookup,
                    &inner_send_result,
                )
                .await
            }
        });
    }
    for parent_pointer in &parents {
        services.spawn({
            let inner_send_result = send_result.clone();
            let inner_namespace_lookup = namespace_lookup.clone();
            let inner_status_topic = status_topic.clone();
            let inner_child = child.clone();
            let inner_child_output_dir = child_output_dir.clone();
            let inner_child_kernel = Arc::clone(&child_kernel);
            let inner_agent_client = Arc::clone(&agent_client);
            let inner_child_active_packets = child_active_packets.clone();
            let parent = parent_pointer.clone();
            async move {
                start_packet_processor(
                    &inner_agent_client,
                    &inner_status_topic,
                    "output",
                    &parent,
                    &inner_child,
                    &inner_child_output_dir,
                    &inner_child_kernel,
                    inner_child_active_packets,
                    &inner_namespace_lookup,
                    &inner_send_result,
                )
                .await
            }
        });
    }
    services.spawn(async move {
        let mut active_parents = parents.clone();
        while let Some(response) = response_rx.recv().await {
            let child_topic = format!("{status_topic}/output/{child}");
            match response {
                Err(error) => {
                    agent_client
                        .publish(
                            &child_topic,
                            &Payload::<Packet, ()>::Failed(error.to_string()),
                        )
                        .await?;
                    return Ok((child, NodeState::Failed(error.to_string())));
                }
                Ok(payload) => {
                    match &payload {
                        Payload::Failed(_) => todo!("Should not be possible."),
                        Payload::Cancelled => {
                            agent_client
                                .publish(&child_topic, &Payload::<Packet, ()>::Cancelled)
                                .await?;
                            return Ok((child, NodeState::Cancelled));
                        }
                        Payload::End(completed_source) => {
                            // skips if parent is self i.e. parents = vec![] for input nodes
                            if let Some(parent_index) = active_parents
                                .iter()
                                .position(|parent| parent == completed_source)
                            {
                                active_parents.remove(parent_index);
                            }
                        }
                        Payload::Stream(packets) => {
                            for packet in packets {
                                agent_client
                                    .publish(
                                        &child_topic,
                                        &Payload::Stream::<_, ()>(packet.clone()),
                                    )
                                    .await?; // this could be parallelized later with minimal gain
                            }
                        }
                    }
                    if active_parents.is_empty()
                        && child_active_packets.is_empty()
                        && response_rx.is_empty()
                    {
                        agent_client
                            .publish(&child_topic, &Payload::<Packet, _>::End(()))
                            .await?;
                        break;
                    }
                }
            }
        }
        Ok((child, NodeState::Completed))
    });

    services
        .join_next()
        .await
        .context(selector::NoRemainingServices {})??
}

#[expect(clippy::excessive_nesting, reason = "debug")]
async fn start_packet_processor<F, R>(
    agent_client: &Arc<AgentClient>,
    status_topic: &str,
    feed_type: &str,
    source_node: &str,
    node: &str,
    node_output_dir: &URI,
    kernel: &Arc<ActualKernel>,
    active_packets: TaskTracker,
    namespace_lookup: &HashMap<String, PathBuf>,
    send_result: &F,
) -> Result<(String, NodeState)>
where
    F: FnOnce(Result<Payload<Vec<Packet>, String>>) -> R + Clone + Send + Sync + 'static,
    R: Future<Output = Result<()>> + Send,
{
    let subscriber = agent_client
        .session
        .declare_subscriber(format!(
            "group/{}/{status_topic}/{feed_type}/{source_node}/**",
            agent_client.group
        ))
        .await
        .context(selector::AgentCommunicationFailure {})?;
    while let Ok(sample) = subscriber.recv_async().await {
        let inner_agent_client = Arc::clone(agent_client);
        let inner_node_output_dir = node_output_dir.clone();
        if let Ok(payload) =
            serde_json::from_slice::<Payload<Packet, ()>>(&sample.payload().to_bytes())
        {
            match payload {
                Payload::End(()) => {
                    send_result.clone()(Ok(Payload::End(source_node.to_owned()))).await?;
                }
                Payload::Failed(_) | Payload::Cancelled => {
                    send_result.clone()(Ok(Payload::Cancelled)).await?;
                }
                Payload::Stream(packet) => {
                    match &**kernel {
                        ActualKernel::Pod(pod) => active_packets.spawn({
                            let inner_pod = Arc::clone(pod);
                            let inner_namespace_lookup = namespace_lookup.clone();
                            let inner_send_result = send_result.clone();
                            async move {
                                let mut bytes = [0; 32];
                                rand::rng().fill_bytes(&mut bytes);
                                process_pod_packet(
                                    inner_agent_client,
                                    packet,
                                    &inner_pod,
                                    URI {
                                        path: inner_node_output_dir.path.join(hex::encode(bytes)),
                                        ..inner_node_output_dir
                                    },
                                    &inner_namespace_lookup,
                                    "*/pod_job/**".into(),
                                )
                                .map_ok(Payload::Stream)
                                .then(inner_send_result)
                                .await
                            }
                        }),
                        // undefined state if an operator is an input node
                        ActualKernel::JoinOperator(operator) => active_packets.spawn({
                            let packet_response = process_operator_packet(
                                source_node.to_owned(),
                                packet,
                                &Arc::clone(operator),
                            )
                            .map(Payload::Stream);
                            let inner_send_result = send_result.clone();
                            async move { inner_send_result(packet_response).await }
                        }),
                        ActualKernel::MapOperator(operator) => active_packets.spawn({
                            let packet_response = process_operator_packet(
                                source_node.to_owned(),
                                packet,
                                &Arc::clone(operator),
                            )
                            .map(Payload::Stream);
                            let inner_send_result = send_result.clone();
                            async move { inner_send_result(packet_response).await }
                        }),
                    };
                }
            }
        }
    }
    Ok((node.to_owned(), NodeState::Idle))
}

#[expect(clippy::unwrap_used, reason = "debug")]
fn process_operator_packet(
    parent: String,
    packet: Packet,
    operator: &Arc<Mutex<impl Operator>>,
) -> Result<Vec<Packet>> {
    operator.lock().unwrap().next(vec![(parent, packet)])
}

#[expect(clippy::excessive_nesting, reason = "Nesting manageable.")]
async fn process_pod_packet(
    agent_client: Arc<AgentClient>,
    packet: Packet,
    pod: &Arc<Pod>,
    output_dir: URI,
    namespace_lookup: &HashMap<String, PathBuf>,
    pod_result_topic_key_expression: String,
) -> Result<Vec<Packet>> {
    let pod_job = PodJob::new(
        Arc::clone(pod),
        packet.into(),
        output_dir,
        pod.recommended_cpus,
        pod.recommended_memory,
        namespace_lookup,
        None,
        None,
    )?;

    let mut services = JoinSet::new();
    services.spawn({
        let inner_agent_client = Arc::clone(&agent_client);
        let inner_pod_result_topic_key_expression = pod_result_topic_key_expression;
        let pod_job_hash = pod_job.hash.clone();
        async move {
            let pod_result;
            let subscriber = inner_agent_client
                .session
                .declare_subscriber(format!(
                    "group/{}/{inner_pod_result_topic_key_expression}",
                    inner_agent_client.group
                ))
                .await
                .context(selector::AgentCommunicationFailure {})?;
            loop {
                let sample = subscriber
                    .recv_async()
                    .await
                    .context(selector::AgentCommunicationFailure {})?;
                if let (Ok(received_pod_result), Some(metadata)) = (
                    serde_json::from_slice::<PodResult>(&sample.payload().to_bytes()),
                    RE_AGENT_KEY_EXPR.captures(sample.key_expr().as_str()),
                ) {
                    if ["success", "failure"].contains(&&metadata["action"])
                        && received_pod_result.pod_job.hash == pod_job_hash
                    {
                        pod_result = received_pod_result;
                        break;
                    }
                }
            }

            match pod_result.status {
                PodStatus::Completed => Ok::<_, OrcaError>(pod_result),
                PodStatus::Failed { exit_code } => Err(selector::PodFailed {
                    hash: pod_result.hash,
                    exit_code,
                }
                .fail()?),
                PodStatus::Running | PodStatus::Unset => todo!("Should not be possible"),
            }
        }
    });

    agent_client.start_pod_jobs(vec![pod_job.into()]).await; // should check that it is equal to `vec![Response::Ok]`

    let pod_result = services
        .join_next()
        .await
        .context(selector::NoRemainingServices {})???;

    Ok(vec![(*pod_result.output_packet).clone()])
}
