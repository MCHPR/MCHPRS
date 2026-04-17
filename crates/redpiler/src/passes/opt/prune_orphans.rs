//! # [`PruneOrphans`]
//!
//! This pass removes any nodes in the graph that aren't transitively connected to an output
//! redstone component by using Depth-First-Search.

use crate::compile_graph::{CompileGraph, Direction};
use crate::passes::{AnalysisInfos, Pass};
use crate::{CompilerInput, CompilerOptions};
use itertools::Itertools;
use mchprs_world::World;

pub struct PruneOrphans;

impl<W: World> Pass<W> for PruneOrphans {
    fn run_pass(
        &self,
        graph: &mut CompileGraph,
        _: &CompilerOptions,
        _: &CompilerInput<'_, W>,
        _: &mut AnalysisInfos,
    ) {
        // We start searching from output nodes
        let mut worklist = graph
            .node_indices()
            .filter(|&idx| graph[idx].is_output)
            .collect_vec();

        let mut visited = vec![false; graph.node_bound()];

        // Visit initial nodes
        for &idx in &worklist {
            visited[idx.index()] = true;
        }

        while let Some(idx) = worklist.pop() {
            for incoming in graph.neighbors(idx, Direction::Incoming) {
                if !visited[incoming.index()] {
                    visited[incoming.index()] = true;
                    worklist.push(incoming);
                }
            }
        }

        graph.retain_nodes(|_, idx| visited[idx.index()]);
    }

    fn status_message(&self) -> &'static str {
        "Pruning orphans"
    }

    fn driver_key(&self) -> &'static str {
        "prune-orphans"
    }
}
