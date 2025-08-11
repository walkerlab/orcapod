use crate::uniffi::{error::Result, model::packet::Packet};
use itertools::Itertools as _;
use serde::{Deserialize, Serialize};
use std::{clone::Clone as _, collections::HashMap, iter::IntoIterator as _, sync::Arc};
use tokio::{sync::Mutex, task::JoinSet};

#[allow(async_fn_in_trait, reason = "We only use this internally")]
pub trait Operator {
    async fn process_packets(&self, packets: Vec<(String, Packet)>) -> Result<Vec<Packet>>;
}

pub struct JoinOperator {
    parent_count: usize,
    received_packets: Arc<Mutex<HashMap<String, Vec<Packet>>>>,
}

impl JoinOperator {
    pub fn new(parent_count: usize) -> Self {
        Self {
            parent_count,
            received_packets: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn process_packet(
        parent_count: usize,
        received_packets: Arc<Mutex<HashMap<String, Vec<Packet>>>>,
        parent_id: String,
        packet: Packet,
    ) -> Vec<Packet> {
        let mut received_packet_lock = received_packets.lock().await;
        received_packet_lock
            .entry(parent_id.clone())
            .or_insert_with(|| vec![packet.clone()])
            .push(packet.clone());

        // If we still don't have at least 1 packet for each parent, skip computation
        if received_packet_lock.len() < parent_count {
            return vec![];
        }

        // Build the factors for the cartesian product, silently missing key since it shouldn't be possible
        let factors = received_packet_lock
            .iter()
            .filter_map(|(id, parent_packets)| (*id != parent_id).then_some(parent_packets.clone()))
            .chain(vec![vec![packet]])
            .collect::<Vec<_>>();

        // We don't need the lock after this point, thus release for other tasks
        drop(received_packet_lock);

        // Compute the cartesian product of the factors, this might take a while
        factors
            .into_iter()
            .multi_cartesian_product()
            .map(|packets_to_combined| {
                packets_to_combined
                    .into_iter()
                    .flat_map(IntoIterator::into_iter)
                    .collect::<HashMap<_, _>>()
            })
            .collect::<Vec<_>>()
    }
}

impl Operator for JoinOperator {
    async fn process_packets(&self, packets: Vec<(String, Packet)>) -> Result<Vec<Packet>> {
        let mut processing_task = JoinSet::new();

        for (parent_id, packet) in packets {
            processing_task.spawn(Self::process_packet(
                self.parent_count,
                Arc::clone(&self.received_packets),
                parent_id,
                packet,
            ));
        }

        let mut new_packets = vec![];
        while let Some(product_packets) = processing_task.join_next().await {
            new_packets.extend(product_packets?);
        }

        Ok(new_packets)
    }
}

#[derive(uniffi::Object, Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct MapOperator {
    map: HashMap<String, String>,
}

impl Operator for MapOperator {
    async fn process_packets(&self, packets: Vec<(String, Packet)>) -> Result<Vec<Packet>> {
        Ok(packets
            .into_iter()
            .map(|(_, packet)| {
                packet
                    .iter()
                    .map(|(packet_key, path_set)| {
                        (
                            self.map
                                .get(packet_key)
                                .map_or_else(|| packet_key.clone(), Clone::clone),
                            path_set.clone(),
                        )
                    })
                    .collect()
            })
            .collect::<Vec<_>>())
    }
}
