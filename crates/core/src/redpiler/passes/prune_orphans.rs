//! # [`PruneOrphans`]
//!
//! This pass removes any nodes in the graph that aren't transitively connected to an output redstone component by using Depth-First-Search.

use super::Pass;
use crate::redpiler::compile_graph::CompileGraph;
use crate::redpiler::{CompilerInput, CompilerOptions};
use crate::world::World;
use petgraph::Direction;
use rustc_hash::FxHashSet;

pub struct PruneOrphans;

impl<W: World> Pass<W> for PruneOrphans {
    fn run_pass(&self, graph: &mut CompileGraph, _: &CompilerOptions, _: &CompilerInput<'_, W>) {
        let mut to_visit = Vec::with_capacity(graph.node_count());
        for idx in graph.node_indices() {
            if graph[idx].is_output {
                to_visit.push(idx);
            }
        }

        let mut visited = FxHashSet::default();
        while let Some(idx) = to_visit.pop() {
            if !visited.contains(&idx) {
                visited.insert(idx);
                for neighbor in graph.neighbors_directed(idx, Direction::Incoming) {
                    to_visit.push(neighbor);
                }
            }
        }

        graph.retain_nodes(|_, n| visited.contains(&n));
    }
}
