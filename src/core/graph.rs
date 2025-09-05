use crate::{
    core::{model::pipeline::PipelineNode, util::get},
    uniffi::{error::Result, model::pipeline::Kernel},
};
use dot_parser::ast::Graph as DOTGraph;
use petgraph::{
    dot::dot_parser::{DotAttrList, DotNodeWeight, ParseFromDot as _},
    graph::DiGraph,
};
use std::collections::HashMap;

#[expect(
    clippy::needless_pass_by_value,
    clippy::panic_in_result_fn,
    clippy::panic,
    reason = "
        - Drop metadata and encourage usage from the graph.
        - `node_map` does not allow returning results.
    "
)]
pub fn make_graph(
    input_dot: &str,
    metadata: HashMap<String, Kernel>,
) -> Result<DiGraph<PipelineNode, ()>> {
    let graph =
        DiGraph::<DotNodeWeight, DotAttrList>::from_dot_graph(DOTGraph::try_from(input_dot)?).map(
            |_, node| PipelineNode {
                hash: node.id.clone(),
                kernel: get(&metadata, &node.id)
                    .unwrap_or_else(|error| panic!("{error}"))
                    .clone(),
            },
            |_, _| (),
        );

    Ok(graph)
}
