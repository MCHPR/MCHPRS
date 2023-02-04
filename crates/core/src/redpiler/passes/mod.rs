mod clamp_weights;
mod coalesce;
mod constant_coalesce;
mod constant_fold;
mod dedup_links;
mod export_graph;
mod identify_nodes;
mod input_search;
mod unreachable_output;

use crate::world::World;

use super::compile_graph::CompileGraph;
use super::{CompilerInput, CompilerOptions};
use std::time::Instant;
use tracing::trace;

pub const fn make_default_pass_manager<'w, W: World>() -> PassManager<'w, W> {
    PassManager::new(&[
        &identify_nodes::IdentifyNodes,
        &input_search::InputSearch,
        &clamp_weights::ClampWeights,
        &dedup_links::DedupLinks,
        &constant_fold::ConstantFold,
        &unreachable_output::UnreachableOutput,
        &constant_coalesce::ConstantCoalesce,
        &coalesce::Coalesce,
        &export_graph::ExportGraph,
    ])
}

pub struct PassManager<'p, W: World> {
    passes: &'p [&'p dyn Pass<W>],
}

impl<'p, W: World> PassManager<'p, W> {
    pub const fn new(passes: &'p [&dyn Pass<W>]) -> Self {
        Self { passes }
    }

    pub fn run_passes(
        &self,
        options: &CompilerOptions,
        input: &CompilerInput<'_, W>,
    ) -> CompileGraph {
        let mut graph = CompileGraph::new();

        for &pass in self.passes {
            if !pass.should_run(options) {
                trace!("Skipping pass: {}", pass.name());
                continue;
            }

            trace!("Running pass: {}", pass.name());
            let start = Instant::now();

            pass.run_pass(&mut graph, options, input);

            trace!("Completed pass in {:?}", start.elapsed());
            trace!("node_count: {}", graph.node_count());
            trace!("edge_count: {}", graph.edge_count());
        }

        graph
    }
}

pub trait Pass<W: World> {
    fn run_pass(
        &self,
        graph: &mut CompileGraph,
        options: &CompilerOptions,
        input: &CompilerInput<'_, W>,
    );

    /// This name should only be use for debugging purposes,
    /// it is not a valid identifier of the pass.
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    fn should_run(&self, options: &CompilerOptions) -> bool {
        // Run passes for optimized builds by default
        options.optimize
    }
}
