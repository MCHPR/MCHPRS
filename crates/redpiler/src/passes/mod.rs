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

use mchprs_world::World;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::ril::DumpGraph;

use super::compile_graph::CompileGraph;
use super::task_monitor::TaskMonitor;
use super::{CompilerInput, CompilerOptions};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, trace};

use analysis::*;

pub fn build_pass_pipeline<'p, W: World>(
    registry: &'p PassRegistry<W>,
    options: &CompilerOptions,
) -> PassPipeline<'p, W> {
    let mut builder = PassPipelineBuilder::new(&registry);

    builder.add_pass::<identify_nodes::IdentifyNodes>();
    builder.add_pass::<input_search::InputSearch>();
    builder.add_pass::<clamp_weights::ClampWeights>();

    if options.optimize {
        builder.add_pass::<dedup_links::DedupLinks>();
        builder.add_pass::<constant_fold::ConstantFold>();
        builder.add_pass::<unreachable_output::UnreachableOutput>();
        builder.add_pass::<constant_coalesce::ConstantCoalesce>();
        builder.add_pass::<coalesce::Coalesce>();
        if options.io_only {
            builder.add_pass::<prune_orphans::PruneOrphans>();
        }
    }

    if options.export {
        builder.add_pass::<export_graph::ExportGraph>();
    }

    builder.build()
}

pub struct PassRegistry<W: World> {
    passes: FxHashMap<TypeId, Box<dyn Pass<W>>>,
    driver_name_map: FxHashMap<&'static str, TypeId>,
}

impl<W: World> Default for PassRegistry<W> {
    fn default() -> Self {
        let mut registry = Self::new();

        // Frontend passes
        registry.register_pass(identify_nodes::IdentifyNodes);
        registry.register_pass(input_search::InputSearch);

        // Analysis Passes
        registry.register_pass(ss_range_analysis::SSRangeAnalysis);

        // Optimization Passes
        registry.register_pass(clamp_weights::ClampWeights);
        registry.register_pass(dedup_links::DedupLinks);
        registry.register_pass(constant_fold::ConstantFold);
        registry.register_pass(unreachable_output::UnreachableOutput);
        registry.register_pass(constant_coalesce::ConstantCoalesce);
        registry.register_pass(coalesce::Coalesce);
        registry.register_pass(prune_orphans::PruneOrphans);
        registry.register_pass(export_graph::ExportGraph);

        registry
    }
}

impl<W: World> PassRegistry<W> {
    pub fn new() -> Self {
        Self {
            passes: FxHashMap::default(),
            driver_name_map: FxHashMap::default(),
        }
    }

    pub fn register_pass<P: Pass<W>>(&mut self, pass: P) {
        self.driver_name_map
            .insert(pass.driver_key(), pass.type_id());
        if let Some(pass) = self.passes.insert(pass.type_id(), Box::new(pass)) {
            panic!("registered duplicate pass: {}", pass.debug_name())
        }
    }

    pub fn get_pass<P: Pass<W>>(&self) -> &dyn Pass<W> {
        self.get_pass_from_id(TypeId::of::<P>())
    }

    pub fn get_pass_from_id(&self, id: TypeId) -> &dyn Pass<W> {
        &*self.passes[&id]
    }

    pub fn get_pass_from_driver_key(&self, key: &str) -> Option<&dyn Pass<W>> {
        self.driver_name_map
            .get(key)
            .map(|type_id| self.get_pass_from_id(*type_id))
    }
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
}

pub struct PassPipelineBuilder<'p, W: World> {
    registry: &'p PassRegistry<W>,
    passes: Vec<&'p dyn Pass<W>>,
    available_analysis: FxHashSet<TypeId>,
    analysis_usage: AnalysisUsage,
}

impl<'p, W: World> PassPipelineBuilder<'p, W> {
    pub fn new(registry: &'p PassRegistry<W>) -> Self {
        Self {
            registry,
            passes: Vec::new(),
            available_analysis: FxHashSet::default(),
            analysis_usage: AnalysisUsage::default(),
        }
    }

    /// Returns true if the pass was found
    pub fn add_pass_by_driver_key(&mut self, driver_key: &str) -> bool {
        let Some(pass) = self.registry.get_pass_from_driver_key(driver_key) else {
            return false;
        };
        self.add_pass_by_instance(pass);
        true
    }

    pub fn add_pass<P: Pass<W>>(&mut self) {
        let pass = self.registry.get_pass::<P>();
        self.add_pass_by_instance(pass);
    }

    pub fn add_pass_by_instance(&mut self, pass: &'p dyn Pass<W>) {
        let au = &mut self.analysis_usage;
        au.reset();

        pass.analysis_usage(au);
        for type_id in &au.required {
            if !self.available_analysis.contains(type_id) {
                let analysis_pass = self.registry.get_pass_from_id(*type_id);
                self.passes.push(analysis_pass);
            }
        }

        self.passes.push(pass);

        if !au.preserves_all {
            self.available_analysis
                .retain(|type_id| au.preserved.contains(type_id));
        }
    }

    pub fn build(self) -> PassPipeline<'p, W> {
        PassPipeline {
            passes: self.passes,
        }
    }
}

pub struct PassPipeline<'p, W: World> {
    passes: Vec<&'p dyn Pass<W>>,
}

impl<'p, W: World> PassPipeline<'p, W> {
    pub fn run_passes(
        &self,
        options: &CompilerOptions,
        input: &CompilerInput<'_, W>,
        mut graph: CompileGraph,
        monitor: Arc<TaskMonitor>,
    ) -> CompileGraph {
        // Add one for the backend compile step
        monitor.set_max_progress(self.passes.len() + 1);

        let mut analysis_infos = AnalysisInfos::default();

        for &pass in &self.passes {
            if monitor.cancelled() {
                return graph;
            }

            trace!("Running pass: {}", pass.debug_name());
            monitor.set_message(pass.status_message().to_string());
            let start = Instant::now();

            pass.run_pass(&mut graph, options, input, &mut analysis_infos);

            trace!("Completed pass in {:?}", start.elapsed());
            trace!("node_count: {}", graph.node_count());
            trace!("edge_count: {}", graph.edge_count());
            monitor.inc_progress();

            if options.print_after_all {
                debug!("Printing circuit after pass: {}", pass.debug_name());
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

#[derive(Default)]
pub struct AnalysisUsage {
    preserves_all: bool,
    required: Vec<TypeId>,
    preserved: Vec<TypeId>,
}

impl AnalysisUsage {
    fn reset(&mut self) {
        self.preserves_all = false;
        self.required.clear();
        self.preserved.clear();
    }

    pub fn set_preserves_all(&mut self) {
        self.preserves_all = true;
    }

    pub fn set_required<P: Pass<W>, W: World>(&mut self) {
        self.required.push(TypeId::of::<P>());
    }

    pub fn set_preserved<P: Pass<W>, W: World>(&mut self) {
        self.preserved.push(TypeId::of::<P>());
    }
}

pub trait Pass<W: World>: 'static {
    fn run_pass(
        &self,
        graph: &mut CompileGraph,
        options: &CompilerOptions,
        input: &CompilerInput<'_, W>,
        analysis_infos: &mut AnalysisInfos,
    );

    /// This name should only be use for debugging purposes,
    /// it is not a valid identifier of the pass.
    fn debug_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    fn analysis_usage(&self, _au: &mut AnalysisUsage) {}

    fn status_message(&self) -> &'static str;

    /// A kebab-case identifier for this pass. Used by rilc.
    fn driver_key(&self) -> &'static str;
}
