use crate::{
    core::util::get,
    uniffi::{error::Result, model::packet::Packet},
};
use itertools::Itertools as _;
use std::{clone::Clone as _, collections::HashMap, iter::IntoIterator as _};

pub trait Operator {
    fn next(&mut self, packets: Vec<(String, Packet)>) -> Result<Vec<Packet>>;
}

pub struct JoinOperator {
    parent_count: usize,
    packet_cache: HashMap<String, Vec<Packet>>,
}

impl JoinOperator {
    pub fn new(parent_count: usize) -> Self {
        Self {
            parent_count,
            packet_cache: HashMap::new(),
        }
    }
}

#[expect(clippy::excessive_nesting, reason = "Nesting manageable.")]
impl Operator for JoinOperator {
    fn next(&mut self, packets: Vec<(String, Packet)>) -> Result<Vec<Packet>> {
        let temp: Vec<Packet> = packets
            .iter()
            .flat_map(|(parent_id, packet)| {
                self.packet_cache
                    .entry(parent_id.clone())
                    .or_insert_with(|| vec![packet.clone()])
                    .push(packet.clone());

                // If we still don't have at least 1 packet for each parent, skip computation
                if self.packet_cache.len() < self.parent_count {
                    return vec![];
                }

                // Build the factors for the cartesian product, silently missing key since it shouldn't be possible
                let factors = self
                    .packet_cache
                    .iter()
                    .filter_map(|(id, parent_packets)| {
                        (id != parent_id).then_some(parent_packets.clone())
                    })
                    .chain(vec![vec![packet.clone()]]);

                factors
                    .multi_cartesian_product()
                    .map(|packets_to_combined| {
                        packets_to_combined.into_iter().fold(
                            HashMap::new(),
                            |mut acc, new_packet| {
                                acc.extend(new_packet);
                                acc
                            },
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        Ok(temp)
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
    fn next(&mut self, packets: Vec<(String, Packet)>) -> Result<Vec<Packet>> {
        Ok(packets
            .iter()
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
            .collect())
    }
}
