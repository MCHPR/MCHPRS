use super::Pass;
use crate::redpiler::compile_graph::CompileGraph;
use crate::redpiler::{CompilerInput, CompilerOptions};
use crate::world::World;

pub struct ClampWeights;

impl<W: World> Pass<W> for ClampWeights {
    fn run_pass(&self, graph: &mut CompileGraph, _: &CompilerOptions, _: &CompilerInput<'_, W>) {
        graph.retain_edges(|g, edge| g[edge].ss < 15);
    }

    fn should_run(&self, _: &CompilerOptions) -> bool {
        // Mandatory
        true
    }
}
