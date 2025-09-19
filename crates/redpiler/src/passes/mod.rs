mod analysis;
mod clamp_weights;
mod coalesce;
mod constant_coalesce;
mod constant_fold;
mod dedup_links;
mod export_graph;
mod identify_nodes;
mod input_search;
mod prune_orphans;
mod unreachable_output;
mod coalesce2;
mod constant_fold2;

use mchprs_world::World;

use crate::ril::DumpGraph;

use super::compile_graph::CompileGraph;
use super::task_monitor::TaskMonitor;
use super::{CompilerInput, CompilerOptions};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, trace};

pub const fn make_default_pass_manager<'w, W: World>() -> PassManager<'w, W> {
    PassManager::new(&[
        &identify_nodes::IdentifyNodes,
        &input_search::InputSearch,
        &clamp_weights::ClampWeights,
        &dedup_links::DedupLinks,
        &analysis::ss_range_analysis::SSRangeAnalysis,
        
        &unreachable_output::UnreachableOutput,
        &constant_fold2::ConstantFold2,
        &coalesce2::Coalesce2,
        
        &prune_orphans::PruneOrphans,
        &export_graph::ExportGraph,
    ])
}

pub trait AnalysisInfo: Any {}

#[derive(Default)]
pub struct AnalysisInfos {
    analysis_infos: HashMap<TypeId, Box<dyn AnalysisInfo>>,
}

impl AnalysisInfos {
    pub fn insert_analysis<A: AnalysisInfo>(&mut self, analysis: A) {
        self.analysis_infos
            .insert(analysis.type_id(), Box::new(analysis));
    }

    pub fn get_analysis<A: AnalysisInfo>(&self) -> Option<&A> {
        let type_id = TypeId::of::<A>();
        self.analysis_infos
            .get(&type_id)
            .and_then(|ai| (ai.as_ref() as &dyn Any).downcast_ref())
    }

    pub fn get_analysis_mut<A: AnalysisInfo>(&mut self) -> Option<&mut A> {
        let type_id = TypeId::of::<A>();
        self.analysis_infos
            .get_mut(&type_id)
            .and_then(|ai| (ai.as_mut() as &mut dyn Any).downcast_mut())
    }
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
        monitor: Arc<TaskMonitor>,
    ) -> CompileGraph {
        let mut graph = CompileGraph::new();

        // Add one for the backend compile step
        monitor.set_max_progress(self.passes.len() + 1);

        let mut analysis_infos = AnalysisInfos::default();

        for &pass in self.passes {
            if !pass.should_run(options) {
                trace!("Skipping pass: {}", pass.name());
                monitor.inc_progress();
                continue;
            }

            if monitor.cancelled() {
                return graph;
            }

            trace!("Running pass: {}", pass.name());
            monitor.set_message(pass.status_message().to_string());
            let start = Instant::now();

            pass.run_pass(&mut graph, options, input, &mut analysis_infos);

            trace!("Completed pass in {:?}", start.elapsed());
            trace!("node_count: {}", graph.node_count());
            trace!("edge_count: {}", graph.edge_count());
            monitor.inc_progress();

            if options.print_after_all {
                debug!("Printing circuit after pass: {}", pass.name());
                graph.dump();
            }
        }

        if options.print_before_backend {
            debug!("Printing circuit before backend compile:");
            graph.dump();
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
        analysis_infos: &mut AnalysisInfos,
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

    fn status_message(&self) -> &'static str;
}
