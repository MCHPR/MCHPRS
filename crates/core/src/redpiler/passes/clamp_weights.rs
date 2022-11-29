use super::Pass;
use crate::redpiler::compile_graph::CompileGraph;
use crate::redpiler::{CompilerInput, CompilerOptions};

pub struct ClampWeights;

impl Pass for ClampWeights {
    fn run_pass(&self, graph: &mut CompileGraph, _: &CompilerOptions, _: &CompilerInput<'_>) {
        graph.retain_edges(|g, edge| g[edge].ss < 15);
    }
}
