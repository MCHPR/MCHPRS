use crate::compile_graph::CompileGraph;
use crate::passes::{AnalysisInfos, Pass};
use crate::{CompilerInput, CompilerOptions};
use mchprs_world::World;

pub struct ClampWeights;

impl<W: World> Pass<W> for ClampWeights {
    fn run_pass(
        &self,
        graph: &mut CompileGraph,
        _: &CompilerOptions,
        _: &CompilerInput<'_, W>,
        _: &mut AnalysisInfos,
    ) {
        graph.retain_edges(|g, edge| g[edge].ss < 15);
    }

    fn status_message(&self) -> &'static str {
        "Clamping weights"
    }

    fn driver_key(&self) -> &'static str {
        "clamp-weights"
    }
}
