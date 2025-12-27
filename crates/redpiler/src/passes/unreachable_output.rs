//! # [`UnreachableOutput`]
//!
//! This pass uses the output of SSRangeAnalysis pass to find links that can be removed because the
//! output ss of a node is never higher than the weight of the link.

use super::Pass;
use crate::compile_graph::{CompileGraph, NodeIdx};
use crate::passes::analysis::ss_range_analysis::SSRangeInfo;
use crate::passes::AnalysisInfos;
use crate::{CompilerInput, CompilerOptions};
use mchprs_world::World;
use petgraph::visit::NodeIndexable;
use petgraph::Direction;

pub struct UnreachableOutput;

impl<W: World> Pass<W> for UnreachableOutput {
    fn run_pass(
        &self,
        graph: &mut CompileGraph,
        _: &CompilerOptions,
        _: &CompilerInput<'_, W>,
        analysis_infos: &mut AnalysisInfos,
    ) {
        let range_info: &SSRangeInfo = analysis_infos.get_analysis().unwrap();

        for i in 0..graph.node_bound() {
            let idx = NodeIdx::new(i);
            if !graph.contains_node(idx) {
                continue;
            }
            let range = range_info.get_range(idx).unwrap();

            // Now we can go through all the outgoing nodes and remove the ones with a weight that
            // is too high.
            let mut outgoing = graph.neighbors_directed(idx, Direction::Outgoing).detach();
            while let Some((edge_idx, _)) = outgoing.next(graph) {
                if graph[edge_idx].ss >= range.high() {
                    graph.remove_edge(edge_idx);
                }
            }
        }
    }

    fn status_message(&self) -> &'static str {
        "Pruning unreachable comparator outputs"
    }
}
