use crate::{
    core::util::get,
    uniffi::{
        error::{Result, selector},
        model::Kernel,
    },
};
use layout::{
    backends::svg::SVGWriter,
    gv::{
        GraphBuilder,
        parser::{DotParser, ast::Stmt},
    },
};
use petgraph::{
    algo::is_cyclic_directed,
    dot::{Config, Dot},
    graph::DiGraph,
    prelude::NodeIndex,
};
use std::collections::HashMap;

#[expect(clippy::excessive_nesting, reason = "Nesting manageable.")]
fn make_graph_from_dot(dot: &str) -> Result<DiGraph<String, String>> {
    let mut petgraph = DiGraph::new();
    let mut node_index_lookup = HashMap::<String, NodeIndex>::new();

    let mut parser = DotParser::new(dot);
    let graph = parser
        .process()
        .map_err(|message| selector::Layout { message }.build())?;

    for stmt in &graph.list.list {
        if let Stmt::Node(node_stmt) = stmt {
            let node_name = &node_stmt.id.name;
            node_index_lookup.insert(node_name.clone(), petgraph.add_node(node_name.clone()));
        }
    }

    for stmt in &graph.list.list {
        if let Stmt::Edge(edge_stmt) = stmt {
            let mut from_node_name = &edge_stmt.from.name;
            if !node_index_lookup.contains_key(from_node_name) {
                node_index_lookup.insert(
                    from_node_name.clone(),
                    petgraph.add_node(from_node_name.clone()),
                );
            }
            for (to_node, _) in &edge_stmt.to {
                let to_node_name = &to_node.name;
                if !node_index_lookup.contains_key(to_node_name) {
                    node_index_lookup.insert(
                        to_node_name.clone(),
                        petgraph.add_node(to_node_name.clone()),
                    );
                }
                petgraph.add_edge(
                    *get(&node_index_lookup, from_node_name)?,
                    *get(&node_index_lookup, to_node_name)?,
                    String::new(),
                );
                from_node_name = &to_node.name;
            }
        }
    }

    Ok(petgraph)
}

fn make_svg_from_dot(dot: &str) -> Result<String> {
    let mut parser = DotParser::new(dot);
    let graph = parser
        .process()
        .map_err(|message| selector::Layout { message }.build())?;

    let mut gb = GraphBuilder::new();
    gb.visit_graph(&graph);
    let mut vg = gb.get();

    let mut svg = SVGWriter::new();
    vg.do_it(false, false, false, &mut svg);
    Ok(svg.finalize())
}
// todo: checks that metadata only contains referenced nodes
pub fn make_graph(
    input_dot: &str,
    input_metadata: &HashMap<String, Kernel>,
) -> Result<(DiGraph<String, String>, HashMap<String, Kernel>)> {
    let graph = make_graph_from_dot(input_dot)?;
    if is_cyclic_directed(&graph) {
        return Err(selector::PipelineCyclic.fail()?);
    }
    Ok((graph, input_metadata.clone()))
}
// todo: should always be topologically sorted, need to contribute to petgraph...
pub fn make_dot(graph: &DiGraph<String, String>) -> String {
    format!(
        "{}",
        Dot::with_attr_getters(
            graph,
            &[Config::NodeNoLabel, Config::EdgeNoLabel],
            &|_graph, _edge| String::new(),
            &|_graph, node| format!("label = \"{}\"", node.1),
        )
    )
}

pub fn make_svg(graph: &DiGraph<String, String>) -> Result<String> {
    let dot = make_dot(graph);
    make_svg_from_dot(&dot)
}
