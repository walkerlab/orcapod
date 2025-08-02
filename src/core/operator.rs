use crate::uniffi::{error::Result, model::packet::Packet};
use itertools::Itertools as _;
use std::{
    clone::Clone,
    collections::HashMap,
    iter::IntoIterator,
    sync::{Arc, Mutex},
};

pub trait Operator {
    fn next(&self, packets: Vec<(String, Packet)>) -> Result<Vec<Packet>>;
}

pub struct JoinOperator {
    parent_count: usize,
    received_streams: Arc<Mutex<HashMap<String, Vec<Packet>>>>,
}

impl JoinOperator {
    pub fn new(parent_count: usize) -> Self {
        Self {
            parent_count,
            received_streams: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[expect(clippy::excessive_nesting, reason = "Nesting manageable.")]
impl Operator for JoinOperator {
    fn next(&self, packets: Vec<(String, Packet)>) -> Result<Vec<Packet>> {
        let mut next_packets = vec![];
        for (stream, packet) in &packets {
            let mut received_streams = self.received_streams.lock()?;
            if self.parent_count - usize::from(!received_streams.contains_key(stream))
                == received_streams.len()
            {
                let packets_to_multiplex = received_streams
                    .iter()
                    .filter_map(|(parent_stream, parent_packets)| {
                        (parent_stream != stream).then_some(parent_packets.clone())
                    })
                    .chain(vec![vec![packet.clone()]].into_iter());
                let current_packets = packets_to_multiplex.multi_cartesian_product().map(
                    |packet_combinations_to_merge| {
                        packet_combinations_to_merge
                            .into_iter()
                            .flat_map(IntoIterator::into_iter)
                            .collect::<HashMap<_, _>>()
                    },
                );
                next_packets.extend(current_packets);
            }
            received_streams
                .entry(stream.clone())
                .or_default()
                .push(packet.clone());
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
    fn next(&self, packets: Vec<(String, Packet)>) -> Result<Vec<Packet>> {
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
