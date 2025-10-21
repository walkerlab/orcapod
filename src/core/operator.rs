use crate::{
    core::model::ToYaml,
    uniffi::{error::Result, model::packet::Packet, operator::MapOperator},
};
use async_trait;
use itertools::Itertools as _;
use std::{clone::Clone, collections::HashMap, iter::IntoIterator, sync::Arc};
use tokio::sync::Mutex;

#[async_trait::async_trait]
pub trait Operator {
    async fn process_packet(&self, stream_name: String, packet: Packet) -> Result<Vec<Packet>>;
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
    async fn process_packet(&self, stream_name: String, packet: Packet) -> Result<Vec<Packet>> {
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

#[async_trait::async_trait]
impl Operator for MapOperator {
    async fn process_packet(&self, _: String, packet: Packet) -> Result<Vec<Packet>> {
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

impl ToYaml for MapOperator {
    fn process_field(
        field_name: &str,
        field_value: &serde_yaml::Value,
    ) -> Option<(String, serde_yaml::Value)> {
        match field_name {
            "hash" => None,
            _ => Some((field_name.to_owned(), field_value.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(clippy::panic_in_result_fn, reason = "OK in tests.")]

    use crate::{
        core::operator::{JoinOperator, MapOperator, Operator},
        uniffi::{
            error::Result,
            model::packet::{Blob, BlobKind, Packet, PathSet, URI},
        },
    };
    use std::{collections::HashMap, path::PathBuf};

    fn make_packet_key(key_name: String, filepath: String) -> (String, PathSet) {
        (
            key_name,
            PathSet::Unary(Blob {
                kind: BlobKind::File,
                location: URI {
                    namespace: "default".into(),
                    path: PathBuf::from(filepath),
                },
                checksum: String::new(),
            }),
        )
    }

    async fn next_batch(
        operator: impl Operator,
        packets: Vec<(String, Packet)>,
    ) -> Result<Vec<Packet>> {
        let mut next_packets = vec![];
        for (stream_name, packet) in packets {
            next_packets.extend(operator.process_packet(stream_name, packet).await?);
        }
        Ok(next_packets)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn join_once() -> Result<()> {
        let operator = JoinOperator::new(2);

        let left_stream = (0..3)
            .map(|i| {
                (
                    "left".into(),
                    Packet::from([make_packet_key(
                        "subject".into(),
                        format!("left/subject{i}.png"),
                    )]),
                )
            })
            .collect::<Vec<_>>();

        let right_stream = (0..2)
            .map(|i| {
                (
                    "right".into(),
                    Packet::from([make_packet_key(
                        "style".into(),
                        format!("right/style{i}.t7"),
                    )]),
                )
            })
            .collect::<Vec<_>>();

        let mut input_streams = left_stream;
        input_streams.extend(right_stream);

        assert_eq!(
            next_batch(operator, input_streams).await?,
            vec![
                Packet::from([
                    make_packet_key("subject".into(), "left/subject0.png".into()),
                    make_packet_key("style".into(), "right/style0.t7".into()),
                ]),
                Packet::from([
                    make_packet_key("subject".into(), "left/subject1.png".into()),
                    make_packet_key("style".into(), "right/style0.t7".into()),
                ]),
                Packet::from([
                    make_packet_key("subject".into(), "left/subject2.png".into()),
                    make_packet_key("style".into(), "right/style0.t7".into()),
                ]),
                Packet::from([
                    make_packet_key("subject".into(), "left/subject0.png".into()),
                    make_packet_key("style".into(), "right/style1.t7".into()),
                ]),
                Packet::from([
                    make_packet_key("subject".into(), "left/subject1.png".into()),
                    make_packet_key("style".into(), "right/style1.t7".into()),
                ]),
                Packet::from([
                    make_packet_key("subject".into(), "left/subject2.png".into()),
                    make_packet_key("style".into(), "right/style1.t7".into()),
                ]),
            ],
            "Unexpected streams."
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn join_spotty() -> Result<()> {
        let operator = JoinOperator::new(2);

        assert_eq!(
            operator
                .process_packet(
                    "right".into(),
                    Packet::from([make_packet_key("style".into(), "right/style0.t7".into())])
                )
                .await?,
            vec![],
            "Unexpected streams."
        );

        assert_eq!(
            operator
                .process_packet(
                    "right".into(),
                    Packet::from([make_packet_key("style".into(), "right/style1.t7".into())])
                )
                .await?,
            vec![],
            "Unexpected streams."
        );

        assert_eq!(
            operator
                .process_packet(
                    "left".into(),
                    Packet::from([make_packet_key(
                        "subject".into(),
                        "left/subject0.png".into()
                    )])
                )
                .await?,
            vec![
                Packet::from([
                    make_packet_key("subject".into(), "left/subject0.png".into()),
                    make_packet_key("style".into(), "right/style0.t7".into()),
                ]),
                Packet::from([
                    make_packet_key("subject".into(), "left/subject0.png".into()),
                    make_packet_key("style".into(), "right/style1.t7".into()),
                ]),
            ],
            "Unexpected streams."
        );

        assert_eq!(
            next_batch(
                operator,
                (1..3)
                    .map(|i| {
                        (
                            "left".into(),
                            Packet::from([make_packet_key(
                                "subject".into(),
                                format!("left/subject{i}.png"),
                            )]),
                        )
                    })
                    .collect::<Vec<_>>()
            )
            .await?,
            vec![
                Packet::from([
                    make_packet_key("subject".into(), "left/subject1.png".into()),
                    make_packet_key("style".into(), "right/style0.t7".into()),
                ]),
                Packet::from([
                    make_packet_key("subject".into(), "left/subject1.png".into()),
                    make_packet_key("style".into(), "right/style1.t7".into()),
                ]),
                Packet::from([
                    make_packet_key("subject".into(), "left/subject2.png".into()),
                    make_packet_key("style".into(), "right/style0.t7".into()),
                ]),
                Packet::from([
                    make_packet_key("subject".into(), "left/subject2.png".into()),
                    make_packet_key("style".into(), "right/style1.t7".into()),
                ]),
            ],
            "Unexpected streams."
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn map_once() -> Result<()> {
        let operator = MapOperator::new(HashMap::from([("key_old".into(), "key_new".into())]))?;

        assert_eq!(
            operator
                .process_packet(
                    "parent".into(),
                    Packet::from([
                        make_packet_key("key_old".into(), "some/key.txt".into()),
                        make_packet_key("subject".into(), "some/subject.txt".into()),
                    ]),
                )
                .await?,
            vec![Packet::from([
                make_packet_key("key_new".into(), "some/key.txt".into()),
                make_packet_key("subject".into(), "some/subject.txt".into()),
            ]),],
            "Unexpected packet."
        );

        Ok(())
    }
}
