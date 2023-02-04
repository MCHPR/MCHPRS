//! # [`UnreachableOutput`]
//!
//! If the side of a comparator in subtract mode is constant, then the maximum output of the
//! comparator is equal to the difference of the maximum side input and the maximum default input.
//! Outgoing edges that have a weight greater than or equal to the maxiumum output of the
//! comparator can be safely removed.

use super::Pass;
use crate::blocks::ComparatorMode;
use crate::redpiler::compile_graph::{CompileGraph, LinkType, NodeIdx, NodeType};
use crate::redpiler::{CompilerInput, CompilerOptions};
use crate::world::World;
use petgraph::visit::{EdgeRef, NodeIndexable};
use petgraph::Direction;

pub struct UnreachableOutput;

impl<W: World> Pass<W> for UnreachableOutput {
    fn run_pass(&self, graph: &mut CompileGraph, _: &CompilerOptions, _: &CompilerInput<'_, W>) {
        for i in 0..graph.node_bound() {
            let idx = NodeIdx::new(i);
            if !graph.contains_node(idx) {
                continue;
            }

            if graph[idx].ty != NodeType::Comparator(ComparatorMode::Subtract) {
                continue;
            }

            // For simiplicity, we always use 15 here. A more complex implementation in the future
            // might want to properly calculate this.
            let max_input: u8 = 15;

            let mut side_inputs = graph
                .edges_directed(idx, Direction::Incoming)
                .filter(|e| e.weight().ty == LinkType::Side);
            let Some(constant_edge) = side_inputs.next() else {
                continue;
            };
            let constant_idx = constant_edge.source();

            // We only accept one constant input for now. In the future we might wan't to coalesce
            // multiple constant inputs together to make this work, most likely in another pass.
            if side_inputs.next().is_some() {
                continue;
            }

            if graph[constant_idx].ty != NodeType::Constant {
                continue;
            }

            let constant = graph[constant_idx].state.output_strength;
            let max_output = max_input.saturating_sub(constant);

            // Now we can go through all the outgoing nodes and remove the ones with a weight that
            // is too high.
            let mut outgoing = graph.neighbors_directed(idx, Direction::Outgoing).detach();
            while let Some((edge_idx, _)) = outgoing.next(graph) {
                if graph[edge_idx].ss >= max_output {
                    graph.remove_edge(edge_idx);
                }
            }
        }
    }
}
