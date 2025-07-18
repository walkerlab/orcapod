use crate::{
    core::util::get,
    uniffi::{
        error::{Result, selector},
        model::Kernel,
    },
};
use dot_parser::ast::Graph as DOTGraph;
use indoc::indoc;
use minijinja::Environment;
use petgraph::{
    algo::toposort,
    dot::{
        Config, Dot,
        dot_parser::{DotAttrList, DotNodeWeight, ParseFromDot as _},
    },
    graph::{DiGraph, NodeIndex},
    visit::IntoNodeReferences as _,
};
use serde::Serialize;
use std::{clone::Clone, collections::HashMap};

// https://tedboy.github.io/jinja2/templ14.html#list-of-builtin-filters
static DOT_JINJA_TEMPLATE: &str = indoc! {r#"
    digraph {
        graph [size="12"]
        {%- if title_text %}
        labelloc = "t"
        label = "\
            {{ title_text | indent(8, False) }}\
        "
        {%- endif %}
        {{ graph | trim }}
        {%- if caption_text %}
        {
            rank = "sink"
            bottomlabel [
                shape = "note"
                label = "\
                    {{ caption_text | indent(16, False) }}\
                "
            ]
        }
        {%- endif %}
    }
"#};

#[derive(Serialize)]
struct DOTJinjaConfig {
    title_text: Option<String>,
    graph: String,
    caption_text: Option<String>,
}

pub struct DOTAttribute {
    pub color: String,
    pub extra_label: String,
}

fn sort_alphabetically<N: Ord, E>(graph: &DiGraph<N, E>) -> Vec<NodeIndex> {
    let mut node_references = graph.node_references().collect::<Vec<_>>();
    node_references.sort_by(|(_, node_weight1), (_, node_weight2)| node_weight2.cmp(node_weight1));
    node_references
        .into_iter()
        .map(|(node_id, _)| node_id)
        .collect::<Vec<_>>()
}

#[expect(clippy::needless_pass_by_value, reason = "debug")]
fn cast_graph<N: Clone, E: Default>(
    base_node_index_order_iter: impl IntoIterator<Item = NodeIndex> + Clone,
    base_graph: DiGraph<N, E>,
) -> Result<DiGraph<N, E>> {
    let mut new_graph = DiGraph::new();
    let mut base_to_new = HashMap::new();
    for base_node_id in base_node_index_order_iter.clone() {
        if let Some(base_node_weight) = base_graph.node_weight(base_node_id) {
            let new_node_id = new_graph.add_node(base_node_weight.clone());
            base_to_new.insert(base_node_id, new_node_id);
        }
    }
    for base_source in base_node_index_order_iter {
        for base_target in base_graph.neighbors(base_source) {
            new_graph.add_edge(
                *get(&base_to_new, &base_source)?,
                *get(&base_to_new, &base_target)?,
                E::default(),
            );
        }
    }
    Ok(new_graph)
}
// todo: checks that metadata only contains referenced nodes
pub fn make_graph(input_dot: &str) -> Result<DiGraph<String, String>> {
    let mut graph =
        DiGraph::<DotNodeWeight, DotAttrList>::from_dot_graph(DOTGraph::try_from(input_dot)?)
            .map(|_, node| node.id.clone(), |_, _| String::new());

    graph = cast_graph(sort_alphabetically(&graph), graph)?;
    graph = cast_graph(
        toposort(&graph, None).map_err(|cycle| selector::PipelineCyclic { cycle }.build())?,
        graph,
    )?;
    Ok(graph)
}
// todo: remove hashmap index since it can cause panic...
pub fn make_dot(
    graph: &DiGraph<String, String>,
    metadata: &HashMap<String, Kernel>,
    title_text: Option<String>,
    node_attributes_config: Option<&HashMap<String, DOTAttribute>>,
    caption_text: Option<String>,
) -> Result<String> {
    let mut env = Environment::new();
    env.add_template("dot", DOT_JINJA_TEMPLATE)?;
    let template = env.get_template("dot")?;
    Ok(template.render(&DOTJinjaConfig {
        title_text,
        graph: Dot::with_attr_getters(
            graph,
            &[
                Config::NodeNoLabel,
                Config::EdgeNoLabel,
                Config::GraphContentOnly,
            ],
            &|_graph, _edge| String::new(),
            &|_graph, node| {
                node_attributes_config.map_or_else(
                    || {
                        format!(
                            r#"label = "{}" shape = "{}" style = "bold""#,
                            node.1,
                            match metadata[node.1] {
                                Kernel::Pod { .. } => "box",
                                Kernel::MapOperator { .. } | Kernel::JoinOperator => "circle",
                            }
                        )
                    },
                    |node_attributes| {
                        format!(
                            r#"label = "{} [{}]" shape = "{}" style = "bold" color = "{}""#,
                            node.1,
                            node_attributes[node.1].extra_label,
                            match metadata[node.1] {
                                Kernel::Pod { .. } => "box",
                                Kernel::MapOperator { .. } | Kernel::JoinOperator => "circle",
                            },
                            node_attributes[node.1].color,
                        )
                    },
                )
            },
        )
        .to_string(),
        caption_text,
    })?)
}
