//! # [`DedupLinks`]
//!
//! This pass removes duplicate edges from the graph, or parallel edges that have higher weight.
//!
//! For example, if two nodes are connected with two links of weights 13 and 15, the link with
//! weight 15 is removed.

use super::Pass;
use crate::redpiler::compile_graph::{CompileGraph, NodeIdx};
use crate::redpiler::{CompilerInput, CompilerOptions};
use crate::world::World;
use petgraph::visit::{EdgeRef, NodeIndexable};
use petgraph::Direction;

pub struct DedupLinks;

impl<W: World> Pass<W> for DedupLinks {
    fn run_pass(&self, graph: &mut CompileGraph, _: &CompilerOptions, _: &CompilerInput<'_, W>) {
        for i in 0..graph.node_bound() {
            let idx = NodeIdx::new(i);
            if !graph.contains_node(idx) {
                continue;
            }

            let mut edges = graph.neighbors_directed(idx, Direction::Incoming).detach();
            while let Some(edge_idx) = edges.next_edge(graph) {
                let edge = &graph[edge_idx];
                let source_idx = graph.edge_endpoints(edge_idx).unwrap().0;

                let mut should_remove = false;
                for other_edge in graph.edges_directed(idx, Direction::Incoming) {
                    if other_edge.id() != edge_idx
                        && other_edge.source() == source_idx
                        && other_edge.weight().ty == edge.ty
                        && other_edge.weight().ss <= edge.ss
                    {
                        should_remove = true;
                        break;
                    }
                }

                if should_remove {
                    graph.remove_edge(edge_idx);
                }
            }
        }
    }
}
