use std::{sync::Arc, time::Instant};
use mchprs_backend_lib::{BackendVariant, CompilerInput, CompilerOptions, JITBackend, TaskMonitor, compile_graph::CompileGraph, passes::{self, PassRegistry}};
use mchprs_blocks::BlockPos;
use mchprs_world::{TickEntry, World, for_each_block_mut_optimized};


use tracing::{debug, error, trace};

#[enum_delegate::implement(JITBackend,
    pub trait JITBackend {
        fn compile(
            &mut self,
            graph: CompileGraph,
            ticks: Vec<TickEntry>,
            options: &CompilerOptions,
            monitor: Arc<TaskMonitor>,
        );
        fn tick(&mut self);

        fn tickn(&mut self, ticks: u64) {
            for _ in 0..ticks {
                self.tick();
            }
        }

        fn on_use_block(&mut self, pos: BlockPos);
        fn set_pressure_plate(&mut self, pos: BlockPos, powered: bool);
        fn flush<W: World>(&mut self, world: &mut W, io_only: bool);
        fn reset<W: World>(&mut self, world: &mut W, io_only: bool);
        fn has_pending_ticks(&self) -> bool;
        /// Inspect block for debugging
        fn inspect(&mut self, pos: BlockPos);
    }
)]
pub enum BackendDispatcher {
    DirectBackend(mchprs_backend_direct::DirectBackend),
}

#[derive(Default)]
pub struct Compiler {
    is_active: bool,
    backend: Option<BackendDispatcher>,
    options: CompilerOptions,
}

impl Compiler {
    pub fn is_active(&self) -> bool {
        self.is_active
    }

    pub fn current_flags(&self) -> Option<&CompilerOptions> {
        match self.is_active {
            true => Some(&self.options),
            false => None,
        }
    }

    /// Switches the currently active backend to the one specified by `backend`.
    /// Requires recompilation to take effect.
    pub fn use_backend(&mut self, backend: BackendDispatcher) {
        self.backend = Some(backend);
    }

    pub fn compile<W: World>(
        &mut self,
        world: &W,
        bounds: (BlockPos, BlockPos),
        options: CompilerOptions,
        ticks: Vec<TickEntry>,
        monitor: Arc<TaskMonitor>,
    ) {
        debug!("Starting compile");
        let start = Instant::now();

        let input = CompilerInput { world, bounds };
        let registry = PassRegistry::default();
        let pass_pipeline = passes::build_pass_pipeline::<W>(&registry, &options);
        let graph =
            pass_pipeline.run_passes(&options, &input, CompileGraph::default(), monitor.clone());

        if monitor.cancelled() {
            return;
        }

        let replace_backend = match self.backend {
            Some(BackendDispatcher::DirectBackend(_)) => {
                options.backend_variant != BackendVariant::Direct
            }
            None => true,
        };
        if replace_backend {
            debug!("Switching backend to {:?}", options.backend_variant);
            let backend = match options.backend_variant {
                BackendVariant::Direct => BackendDispatcher::DirectBackend(Default::default()),
            };
            self.use_backend(backend);
        }

        if let Some(backend) = &mut self.backend {
            trace!("Compiling backend");
            monitor.set_message("Compiling backend".to_string());
            let start = Instant::now();

            backend.compile(graph, ticks, &options, monitor.clone());

            monitor.inc_progress();
            trace!("Backend compiled in {:?}", start.elapsed());
        } else {
            error!("Cannot compile without backend variant selected");
        }

        self.options = options;
        self.is_active = true;
        debug!("Compile completed in {:?}", start.elapsed());
    }

    pub fn reset<W: World>(&mut self, world: &mut W, bounds: (BlockPos, BlockPos)) {
        if self.is_active {
            self.is_active = false;
            if let Some(backend) = &mut self.backend {
                backend.reset(world, self.options.io_only)
            }
        }

        if self.options.update {
            let (first_pos, second_pos) = bounds;
            for_each_block_mut_optimized(world, first_pos, second_pos, |world, pos| {
                let block = world.get_block(pos);
                mchprs_redstone::update(block, world, pos);
            });
        }
        self.options = Default::default();
    }

    fn backend(&mut self) -> &mut BackendDispatcher {
        assert!(
            self.is_active,
            "tried to get redpiler backend when inactive"
        );
        if let Some(backend) = &mut self.backend {
            backend
        } else {
            panic!("redpiler is active but is missing backend");
        }
    }

    pub fn tick(&mut self) {
        self.backend().tick();
    }

    pub fn tickn(&mut self, ticks: u64) {
        self.backend().tickn(ticks);
    }

    pub fn on_use_block(&mut self, pos: BlockPos) {
        self.backend().on_use_block(pos);
    }

    pub fn set_pressure_plate(&mut self, pos: BlockPos, powered: bool) {
        self.backend().set_pressure_plate(pos, powered);
    }

    pub fn flush<W: World>(&mut self, world: &mut W) {
        let io_only = self.options.io_only;
        self.backend().flush(world, io_only);
    }

    pub fn inspect(&mut self, pos: BlockPos) {
        if let Some(backend) = &mut self.backend {
            backend.inspect(pos);
        } else {
            debug!("cannot inspect when backend is not running");
        }
    }

    pub fn has_pending_ticks(&mut self) -> bool {
        self.backend().has_pending_ticks()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_options() {
        let input = "-io -u --export";
        let expected_options = CompilerOptions {
            io_only: true,
            optimize: true,
            export: true,
            update: true,
            export_dot_graph: false,
            wire_dot_out: false,
            illegal_states_out: false,
            wire_cross_out: false,
            print_after_all: false,
            print_before_backend: false,
            backend_variant: BackendVariant::default(),
            passes: None,
        };
        let options = CompilerOptions::parse(input);

        assert_eq!(options, expected_options);
    }
}
