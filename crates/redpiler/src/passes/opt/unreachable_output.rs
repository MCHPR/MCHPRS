//! # [`UnreachableOutput`]
//!
//! This pass uses the output of [`SSSetAnalysis`] pass to find links that can be removed because the
//! output ss of a node is never higher than the weight of the link.

use crate::compile_graph::{CompileGraph, Direction, NodeIdx};
use crate::passes::analysis::ss_set_analysis::{SSSetAnalysis, SSSetInfo};
use crate::passes::{AnalysisInfos, AnalysisUsage, Pass};
use crate::{CompilerInput, CompilerOptions};
use mchprs_world::World;

pub struct UnreachableOutput;

impl<W: World> Pass<W> for UnreachableOutput {
    fn run_pass(
        &self,
        graph: &mut CompileGraph,
        _: &CompilerOptions,
        _: &CompilerInput<'_, W>,
        analysis_infos: &mut AnalysisInfos,
    ) {
        let set_info: &SSSetInfo = analysis_infos.get_analysis().unwrap();

        for i in 0..graph.node_bound() {
            let idx = NodeIdx::new(i);
            if !graph.contains_node(idx) {
                continue;
            }
            let max_output = set_info.get_set(idx).unwrap().max();

            // Now we can go through all the outgoing nodes and remove the ones with a weight that
            // is too high.
            let mut outgoing = graph.neighbors(idx, Direction::Outgoing).detach();
            while let Some((edge_idx, _)) = outgoing.next(graph) {
                if graph[edge_idx].ss >= max_output {
                    graph.remove_edge(edge_idx);
                }
            }
        }
    }

    fn analysis_usage(&self, au: &mut AnalysisUsage) {
        au.set_required::<SSSetAnalysis, W>();
    }

    fn status_message(&self) -> &'static str {
        "Pruning unreachable outputs"
    }

    fn driver_key(&self) -> &'static str {
        "unreachable-output"
    }
}
