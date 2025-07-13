use crate::uniffi::{error::Result, model::Packet};
use itertools::Itertools as _;
use std::{clone::Clone, collections::HashMap};

pub trait Operator {
    fn next(&mut self, packets: Vec<(String, Packet)>) -> Result<Vec<Packet>>;
}
// to make this threadsafe, i might need to manage muts within a self mpsc handler
pub struct JoinOperator {
    parent_count: usize,
    received_streams: HashMap<String, Vec<Packet>>,
}

impl JoinOperator {
    pub fn new(parent_count: usize) -> Self {
        Self {
            parent_count,
            received_streams: HashMap::new(),
        }
    }
}

#[expect(clippy::excessive_nesting, reason = "Nesting manageable.")]
impl Operator for JoinOperator {
    fn next(&mut self, packets: Vec<(String, Packet)>) -> Result<Vec<Packet>> {
        let mut next_packets = vec![];
        for (stream, packet) in packets {
            let other_streams_iter = self
                .received_streams
                .iter()
                .filter(|(other_stream, _)| *other_stream != &stream);
            if other_streams_iter.clone().count() + 1 == self.parent_count {
                let mut current_packets =
                    other_streams_iter.fold(vec![packet.clone()], |total, (_, other_packets)| {
                        total
                            .iter()
                            .cartesian_product(other_packets.iter())
                            .map(|(left, right)| {
                                let mut new = HashMap::new();
                                new.extend(left.0.clone());
                                new.extend(right.0.clone());
                                Packet(new)
                            })
                            .collect()
                    });
                next_packets.append(&mut current_packets);
            }
            if let Some(existing_packets) = self.received_streams.get_mut(&stream) {
                existing_packets.push(packet);
            } else {
                self.received_streams.insert(stream, vec![packet]);
            }
        }
        Ok(next_packets)
    }
}

pub struct MapOperator {
    map: HashMap<String, String>,
}

impl MapOperator {
    pub fn new(map: &HashMap<String, String>) -> Self {
        Self { map: map.clone() }
    }
}

impl Operator for MapOperator {
    fn next(&mut self, packets: Vec<(String, Packet)>) -> Result<Vec<Packet>> {
        Ok(packets
            .iter()
            .map(|(_, packet)| {
                Packet(
                    packet
                        .0
                        .iter()
                        .map(|(packet_key, path_set)| {
                            (
                                self.map
                                    .get(packet_key)
                                    .map_or_else(|| packet_key.clone(), Clone::clone),
                                path_set.clone(),
                            )
                        })
                        .collect(),
                )
            })
            .collect())
    }
}
