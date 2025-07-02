use crate::uniffi::{error::Result, model::PathSet};
use itertools::Itertools as _;
use std::collections::HashMap;

type Packet = HashMap<String, PathSet>;

pub trait Operator {
    fn next(&mut self, packets: Vec<(String, Packet)>) -> Result<Vec<Packet>>;
}

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
                                new.extend(left.clone());
                                new.extend(right.clone());
                                new
                            })
                            .collect::<Vec<_>>()
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
