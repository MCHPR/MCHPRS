use super::Pass;
use crate::redpiler::compile_graph::{CompileGraph, LinkType, NodeIdx, NodeType};
use crate::redpiler::{CompilerInput, CompilerOptions};
use petgraph::visit::{EdgeRef, NodeIndexable};
use petgraph::Direction;

pub struct ComparatorOverride;

impl Pass for ComparatorOverride {
    fn run_pass(&self, graph: &mut CompileGraph, _: &CompilerOptions, _: &CompilerInput<'_>) {
        for i in 0..graph.node_bound() {
            let idx = NodeIdx::new(i);
            if !graph.contains_node(idx) {
                continue;
            }

            if !matches!(graph[idx].ty, NodeType::Comparator(_)) {
                continue;
            }

            let is_overriden = graph.edges_directed(idx, Direction::Incoming).any(|edge| {
                graph[edge.source()].ty == NodeType::Constant
                    && edge.weight().ty == LinkType::Default
            });
            if !is_overriden {
                continue;
            }

            let mut edges = graph.neighbors_directed(idx, Direction::Incoming).detach();
            while let Some(edge_idx) = edges.next_edge(graph) {
                let edge = &graph[edge_idx];
                let source_ty = graph[graph.edge_endpoints(edge_idx).unwrap().0].ty;
                if edge.ty == LinkType::Default && source_ty != NodeType::Constant {
                    graph.remove_edge(edge_idx);
                }
            }
        }
    }
}
