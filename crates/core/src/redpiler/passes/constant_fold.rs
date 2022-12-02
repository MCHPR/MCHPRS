use super::Pass;
use crate::redpiler::compile_graph::{CompileGraph, LinkType, NodeIdx, NodeType};
use crate::redpiler::{CompilerInput, CompilerOptions};
use log::trace;
use petgraph::visit::{EdgeRef, NodeIndexable};
use petgraph::Direction;

pub struct ConstantFold;

impl Pass for ConstantFold {
    fn run_pass(&self, graph: &mut CompileGraph, _: &CompilerOptions, _: &CompilerInput<'_>) {
        loop {
            let num_folded = fold(graph);
            if num_folded == 0 {
                break;
            }
            trace!("Fold iteration: {} nodes", num_folded);
        }
    }
}

fn fold(graph: &mut CompileGraph) -> usize {
    let mut num_folded = 0;

    for i in 0..graph.node_bound() {
        let idx = NodeIdx::new(i);
        if !graph.contains_node(idx) {
            continue;
        }

        // TODO: Other node types
        if !matches!(graph[idx].ty, NodeType::Comparator(_)) {
            continue;
        }

        let mut edges = graph.edges_directed(idx, Direction::Incoming);
        let Some(edge) = edges.next() else {
            continue;
        };

        // TODO: Handle multiple inputs
        if edges.next().is_some() {
            continue;
        }

        let constant_idx = edge.source();
        if graph[constant_idx].ty != NodeType::Constant || edge.weight().ty == LinkType::Side {
            continue;
        }

        let mut outgoing = graph.neighbors_directed(idx, Direction::Outgoing).detach();
        while let Some(outgoing_edge) = outgoing.next_edge(graph) {
            let outgoing_node = graph.edge_endpoints(outgoing_edge).unwrap().1;
            let weight = graph.remove_edge(outgoing_edge).unwrap();
            graph.add_edge(constant_idx, outgoing_node, weight);
        }

        graph.remove_node(idx);
        num_folded += 1;
    }

    num_folded
}
