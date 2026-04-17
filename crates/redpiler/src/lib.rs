pub mod backend;
pub mod compile_graph;
pub mod passes;
pub mod ril;
pub mod string_replacer;
pub mod task_monitor;

use backend::{BackendDispatcher, JITBackend};
use mchprs_blocks::blocks::Block;
use mchprs_blocks::BlockPos;
use mchprs_world::{for_each_block_mut_optimized, TickEntry, World};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, trace, warn};

pub use task_monitor::TaskMonitor;

use crate::{compile_graph::CompileGraph, passes::PassRegistry};

fn block_powered_mut(block: &mut Block) -> Option<&mut bool> {
    Some(match block {
        Block::Comparator(comparator) => &mut comparator.powered,
        Block::RedstoneTorch { lit } => lit,
        Block::RedstoneWallTorch { lit, .. } => lit,
        Block::Repeater(repeater) => &mut repeater.powered,
        Block::Lever { powered, .. } => powered,
        Block::StoneButton { powered, .. } => powered,
        Block::RedstoneLamp { lit } => lit,
        Block::IronTrapdoor { powered, .. } => powered,
        Block::NoteBlock { powered, .. } => powered,
        _ => return block.get_pressure_plate_powered(),
    })
}

#[derive(Default, PartialEq, Eq, Debug, Clone)]
pub struct CompilerOptions {
    /// Enable optimization passes which may significantly increase compile times.
    pub optimize: bool,
    /// Export the graph to a binary format. See the [`redpiler_graph`] crate.
    pub export: bool,
    /// Only flush lamp, button, lever, pressure plate, or trapdoor updates.
    pub io_only: bool,
    /// Update all blocks in the input region after reset.
    pub update: bool,
    /// Export a dot file of the graph after backend compile (backend dependent)
    pub export_dot_graph: bool,
    /// Consider a redstone dot to be an output block (for color screens)
    pub wire_dot_out: bool,
    /// Print out the RIL circuit after every redpiler pass
    pub print_after_all: bool,
    /// Print out the RIL circuit before starting backend compile
    pub print_before_backend: bool,
    /// The backend variant to be used after compilation
    pub backend_variant: BackendVariant,
    /// A comma seperated list of passes to run. This can only be used by the rilc driver.
    pub passes: Option<String>,
}

#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub enum BackendVariant {
    #[default]
    Direct,
}

impl CompilerOptions {
    fn parse_option(&mut self, option: &str) {
        if option.starts_with("--") {
            if let Some(passes_str) = option.strip_prefix("--passes=") {
                self.passes = Some(passes_str.to_owned());
                return;
            }

            match option {
                "--optimize" => self.optimize = true,
                "--export" => self.export = true,
                "--io-only" => self.io_only = true,
                "--update" => self.update = true,
                "--export-dot" => self.export_dot_graph = true,
                "--wire-dot-out" => self.wire_dot_out = true,
                "--print-after-all" => self.print_after_all = true,
                "--print-before-backend" => self.print_before_backend = true,
                // FIXME: use actual error handling
                _ => warn!("Unrecognized option: {}", option),
            }
        } else if let Some(str) = option.strip_prefix('-') {
            for c in str.chars() {
                let lower = c.to_lowercase().to_string();
                match lower.as_str() {
                    "o" => self.optimize = true,
                    "e" => self.export = true,
                    "i" => self.io_only = true,
                    "u" => self.update = true,
                    "d" => self.wire_dot_out = true,
                    // FIXME: use actual error handling
                    _ => warn!("Unrecognized option: -{}", c),
                }
            }
        } else {
            // FIXME: use actual error handling
            warn!("Unrecognized option: {}", option);
        }
    }

    pub fn parse(str: &str) -> CompilerOptions {
        let mut co: CompilerOptions = Default::default();
        let options = str.split_whitespace();
        for option in options {
            co.parse_option(option);
        }
        co
    }
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

pub struct CompilerInput<'w, W: World> {
    pub world: &'w W,
    pub bounds: (BlockPos, BlockPos),
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
            print_after_all: false,
            print_before_backend: false,
            backend_variant: BackendVariant::default(),
            passes: None,
        };
        let options = CompilerOptions::parse(input);

        assert_eq!(options, expected_options);
    }
}
