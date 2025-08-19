use crate::uniffi::{error::Result, model::packet::Packet};
use async_trait;
use itertools::Itertools as _;
use std::{clone::Clone, collections::HashMap, iter::IntoIterator, sync::Arc};
use tokio::sync::Mutex;

#[async_trait::async_trait]
pub trait Operator {
    async fn next(&self, stream_name: String, packet: Packet) -> Result<Vec<Packet>>;
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
}

#[async_trait::async_trait]
impl Operator for JoinOperator {
    async fn next(&self, stream_name: String, packet: Packet) -> Result<Vec<Packet>> {
        let mut received_packets = self.received_packets.lock().await;
        received_packets
            .entry(stream_name.clone())
            .or_default()
            .push(packet.clone());
        Ok(
            if self.parent_count - usize::from(!received_packets.contains_key(&stream_name))
                == received_packets.len()
            {
                let packets_to_multiplex = received_packets
                    .iter()
                    .filter_map(|(parent_stream, parent_packets)| {
                        (parent_stream != &stream_name).then_some(parent_packets.clone())
                    })
                    .chain(vec![vec![packet.clone()]])
                    .collect::<Vec<_>>();
                drop(received_packets);

                packets_to_multiplex
                    .into_iter()
                    .multi_cartesian_product()
                    .map(|packet_combinations_to_merge| {
                        packet_combinations_to_merge
                            .into_iter()
                            .flat_map(IntoIterator::into_iter)
                            .collect::<HashMap<_, _>>()
                    })
                    .collect()
            } else {
                vec![]
            },
        )
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

#[async_trait::async_trait]
impl Operator for MapOperator {
    async fn next(&self, _: String, packet: Packet) -> Result<Vec<Packet>> {
        Ok(vec![
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
                .collect(),
        ])
    }
}
