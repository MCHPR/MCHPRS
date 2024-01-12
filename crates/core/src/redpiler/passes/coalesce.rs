use super::Pass;
use crate::redpiler::compile_graph::{CompileGraph, LinkType, NodeIdx, NodeType};
use crate::redpiler::{CompilerInput, CompilerOptions};
use crate::world::World;
use itertools::Itertools;
use petgraph::visit::{EdgeRef, NodeIndexable};
use petgraph::Direction;

pub struct Coalesce;

impl<W: World> Pass<W> for Coalesce {
    fn run_pass(&self, graph: &mut CompileGraph, _: &CompilerOptions, _: &CompilerInput<'_, W>) {
        for i in 0..graph.node_bound() {
            let idx = NodeIdx::new(i);
            if !graph.contains_node(idx) {
                continue;
            }

            let node = &graph[idx];
            // Comparators depend on the link weight as well as the type,
            // we could implement that later if it's beneficial enough.
            if matches!(node.ty, NodeType::Comparator { .. }) || !node.is_removable() {
                continue;
            }

            let Ok(edge) = graph.edges_directed(idx, Direction::Incoming).exactly_one() else {
                continue;
            };

            if edge.weight().ty != LinkType::Default {
                continue;
            }

            let source = edge.source();
            // Comparators might output less than 15 ss
            if matches!(graph[source].ty, NodeType::Comparator { .. }) {
                continue;
            }
            coalesce_outgoing(graph, source, idx);
        }
    }

    fn status_message(&self) -> &'static str {
        "Combining duplicate logic"
    }
}

fn coalesce_outgoing(graph: &mut CompileGraph, source_idx: NodeIdx, into_idx: NodeIdx) {
    let mut walk_outgoing = graph
        .neighbors_directed(source_idx, Direction::Outgoing)
        .detach();
    while let Some(edge_idx) = walk_outgoing.next_edge(graph) {
        let dest_idx = graph.edge_endpoints(edge_idx).unwrap().1;
        if dest_idx == into_idx {
            continue;
        }

        let dest = &graph[dest_idx];
        let into = &graph[into_idx];

        if dest.ty == into.ty
            && dest.is_removable()
            && graph
                .neighbors_directed(dest_idx, Direction::Incoming)
                .count()
                == 1
        {
            coalesce(graph, dest_idx, into_idx);
        }
    }
}

fn coalesce(graph: &mut CompileGraph, node: NodeIdx, into: NodeIdx) {
    let mut walk_outgoing = graph.neighbors_directed(node, Direction::Outgoing).detach();
    while let Some(edge_idx) = walk_outgoing.next_edge(graph) {
        let dest = graph.edge_endpoints(edge_idx).unwrap().1;
        let weight = graph.remove_edge(edge_idx).unwrap();
        graph.add_edge(into, dest, weight);
    }
    graph.remove_node(node);
}
