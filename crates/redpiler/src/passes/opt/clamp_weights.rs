use mchprs_world::World;

use crate::compile_graph::CompileGraph;
use crate::passes::{AnalysisInfos, Pass};
use crate::{CompilerInput, CompilerOptions};

pub struct ClampWeights;

impl<W: World> Pass<W> for ClampWeights {
    fn run_pass(
        &self,
        graph: &mut CompileGraph,
        _: &CompilerOptions,
        _: &CompilerInput<'_, W>,
        _: &mut AnalysisInfos,
    ) {
        graph.retain_edges(|g, edge| g[edge].weight < 15);
    }

    fn status_message(&self) -> &'static str {
        "Clamping weights"
    }

    fn driver_key(&self) -> &'static str {
        "clamp-weights"
    }
}
