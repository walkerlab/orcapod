use crate::uniffi::{error::Result, model::packet::Packet};
use itertools::Itertools as _;
use std::{clone::Clone as _, collections::HashMap, iter::IntoIterator as _, sync::Arc};
use tokio::sync::Mutex;

#[allow(async_fn_in_trait, reason = "We only use this internally")]
pub trait Operator {
    async fn process_packets(&mut self, packets: Vec<(String, Packet)>) -> Result<Vec<Packet>>;
}

pub struct JoinOperator {
    parent_count: usize,
    packet_cache: Arc<Mutex<HashMap<String, Vec<Packet>>>>,
}

impl JoinOperator {
    pub fn new(parent_count: usize) -> Self {
        Self {
            parent_count,
            packet_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[expect(clippy::excessive_nesting, reason = "Nesting manageable.")]
impl Operator for JoinOperator {
    async fn process_packets(&mut self, packets: Vec<(String, Packet)>) -> Result<Vec<Packet>> {
        let mut new_packets = vec![];

        for (parent_id, packet) in packets {
            let mut packet_cache_lock = self.packet_cache.lock().await;
            packet_cache_lock
                .entry(parent_id.clone())
                .or_insert_with(|| vec![packet.clone()])
                .push(packet.clone());

            // If we still don't have at least 1 packet for each parent, skip computation
            if packet_cache_lock.len() < self.parent_count {
                new_packets.extend(vec![]);
                continue;
            }

            // Build the factors for the cartesian product, silently missing key since it shouldn't be possible
            let factors = packet_cache_lock
                .iter()
                .filter_map(|(id, parent_packets)| {
                    (*id != parent_id).then_some(parent_packets.clone())
                })
                .chain(vec![vec![packet]])
                .collect::<Vec<_>>();

            // We don't need the lock after this point
            drop(packet_cache_lock);

            // Compute the cartesian product of the factors, this might take a while
            new_packets.extend(factors.into_iter().multi_cartesian_product().map(
                |packets_to_combined| {
                    packets_to_combined
                        .into_iter()
                        .fold(HashMap::new(), |mut acc, new_packet| {
                            acc.extend(new_packet);
                            acc
                        })
                },
            ));
        }
        Ok(new_packets)
    }
}

pub struct MapOperator {
    map: HashMap<String, String>,
}

impl MapOperator {
    pub const fn new(map: HashMap<String, String>) -> Self {
        Self { map }
    }
}

impl Operator for MapOperator {
    async fn process_packets(&mut self, packets: Vec<(String, Packet)>) -> Result<Vec<Packet>> {
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
