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
static DOT_GRAPH_TEMPLATE: &str = indoc! {r#"
    digraph {
        {%- if is_styled %}
        graph [size="12"]
        {%- endif %}
        {%- if title_text %}
        labelloc = "t"
        label = "\
            {{ title_text | indent(8, False) }}\
        "
        {%- endif %}
        {{ graph_dot | trim }}
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

static DOT_NODE_TEMPLATE: &str = indoc! {r#"
    {%- filter replace("\n", " ") -%}

    {%- if extra_label -%}
    label = "{{ label }} [{{ extra_label }}]"
    {%- else -%}
    label = "{{ label }}"
    {%- endif %}

    {%- if shape %}
    shape = "{{ shape }}"
    {%- endif %}

    {%- if is_styled %}
    style = "bold"
    {%- endif %}
    
    {%- if color %}
    color = "{{ color }}"
    {%- endif %}
    
    {%- endfilter %}
"#};

#[derive(Serialize)]
struct DOTGraphConfig {
    is_styled: bool,
    title_text: Option<String>,
    graph_dot: String,
    caption_text: Option<String>,
}

#[derive(Serialize)]
struct DOTNodeConfig {
    label: String,
    extra_label: Option<String>,
    shape: Option<String>,
    color: Option<String>,
    is_styled: bool,
}

#[derive(Default)]
pub struct DOTStyleConfig<'a> {
    pub title_text: Option<String>, // without omit
    pub node_extra_label: Option<&'a HashMap<String, String>>, // without omit
    pub node_metadata: Option<&'a HashMap<String, Kernel>>, // without no shapes
    pub node_color: Option<&'a HashMap<String, String>>, // without omit
    pub caption_text: Option<String>, // without omit
}

// todo: remove hashmap index since it can cause panic...
#[expect(
    clippy::expect_used,
    reason = "Needed since `Dot::with_attr_getters` doesn't accept results."
)]
pub fn make_dot(
    graph: &DiGraph<String, String>,
    style: Option<DOTStyleConfig>, // without it default to Nones, don't bold shapes, and don't add size graph attribute
) -> Result<String> {
    let (dot_config, is_styled) = style.map_or_else(
        || (DOTStyleConfig::default(), false), // no styling at all
        |config| (config, true),               // bold node shapes, add size to graph + config
    );
    let title_text = dot_config.title_text;
    let caption_text = dot_config.caption_text;
    let node_metadata = dot_config.node_metadata;
    let node_color = dot_config.node_color;
    let node_extra_label = dot_config.node_extra_label;

    let mut env = Environment::new();
    env.add_template("graph", DOT_GRAPH_TEMPLATE)?;
    env.add_template("node", DOT_NODE_TEMPLATE)?;
    let graph_template = env.get_template("graph")?;
    let node_template = env.get_template("node")?;

    let graph_dot = Dot::with_attr_getters(
        graph,
        &[
            Config::NodeNoLabel,
            Config::EdgeNoLabel,
            Config::GraphContentOnly,
        ],
        &|_graph, _edge| String::new(),
        &|_graph, node| {
            node_template
                .render(&DOTNodeConfig {
                    label: node.1.clone(),
                    extra_label: node_extra_label
                        .and_then(|config| config.get(node.1))
                        .cloned(),
                    shape: node_metadata
                        .and_then(|config| config.get(node.1))
                        .map(|kernel| match kernel {
                            Kernel::Pod { .. } => "box".into(),
                            Kernel::MapOperator { .. } | Kernel::JoinOperator => "circle".into(),
                        }),
                    color: node_color.and_then(|config| config.get(node.1)).cloned(),
                    is_styled,
                })
                .expect("Failed to render node.")
        },
    )
    .to_string();

    Ok(graph_template.render(&DOTGraphConfig {
        is_styled,
        title_text,
        graph_dot,
        caption_text,
    })?)
}

fn sort_alphabetically<N: Ord, E>(graph: &DiGraph<N, E>) -> Vec<NodeIndex> {
    let mut node_references = graph.node_references().collect::<Vec<_>>();
    node_references.sort_by(|(_, node_weight1), (_, node_weight2)| node_weight2.cmp(node_weight1));
    node_references
        .into_iter()
        .map(|(node_id, _)| node_id)
        .collect::<Vec<_>>()
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "So we can drop the previous graph and encourage using the new one."
)]
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
