mod backend;
mod compile_graph;
// mod debug_graph;
mod passes;

use crate::blocks::Block;
use crate::redpiler::passes::make_default_pass_manager;
use crate::world::World;
use backend::JITBackend;
use mchprs_blocks::BlockPos;
use mchprs_world::TickEntry;
use std::time::Instant;
use tracing::{debug, error, trace, warn};

use self::backend::BackendDispatcher;

fn bool_to_ss(b: bool) -> u8 {
    match b {
        true => 15,
        false => 0,
    }
}

fn block_powered_mut(block: &mut Block) -> Option<&mut bool> {
    Some(match block {
        Block::RedstoneComparator { comparator } => &mut comparator.powered,
        Block::RedstoneTorch { lit } => lit,
        Block::RedstoneWallTorch { lit, .. } => lit,
        Block::RedstoneRepeater { repeater } => &mut repeater.powered,
        Block::Lever { lever } => &mut lever.powered,
        Block::StoneButton { button } => &mut button.powered,
        Block::StonePressurePlate { powered } => powered,
        Block::RedstoneLamp { lit } => lit,
        Block::IronTrapdoor { powered, .. } => powered,
        _ => return None,
    })
}

#[derive(Default)]
pub struct CompilerOptions {
    pub optimize: bool,
    pub export: bool,
    pub io_only: bool,
}

impl CompilerOptions {
    pub fn parse(str: &str) -> CompilerOptions {
        let mut co: CompilerOptions = Default::default();
        let options = str.split_whitespace();
        for option in options {
            match option {
                "--optimize" | "-O" => co.optimize = true,
                "--export" | "-E" => co.export = true,
                "--io-only" | "-I" => co.io_only = true,
                // FIXME: use actual error handling
                _ => warn!("Unrecognized option: {}", option),
            }
        }
        co
    }
}

#[derive(Default)]
pub struct Compiler {
    is_active: bool,
    jit: Option<BackendDispatcher>,
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

    /// Use just-in-time compilation with a `JITBackend` such as `CraneliftBackend` or `LLVMBackend`.
    /// Requires recompilation to take effect.
    pub fn use_jit(&mut self, jit: BackendDispatcher) {
        self.jit = Some(jit);
    }

    pub fn compile<W: World>(
        &mut self,
        world: &mut W,
        bounds: (BlockPos, BlockPos),
        options: CompilerOptions,
        ticks: Vec<TickEntry>,
    ) {
        debug!("Starting compile");
        let start = Instant::now();

        self.is_active = true;

        let input = CompilerInput { world, bounds };
        let pass_manager = make_default_pass_manager::<W>();
        let graph = pass_manager.run_passes(&options, &input);

        // TODO: Remove this once there is proper backend switching
        if self.jit.is_none() {
            self.use_jit(Default::default());
        }

        if let Some(jit) = &mut self.jit {
            trace!("Compiling backend");
            let start = Instant::now();
            jit.compile(graph, ticks);
            trace!("Backend compiled in {:?}", start.elapsed());
        } else {
            error!("Cannot compile without JIT variant selected");
        }

        self.options = options;
        debug!("Compile completed in {:?}", start.elapsed());
    }

    pub fn reset<W: World>(&mut self, world: &mut W, bounds: (BlockPos, BlockPos)) {
        if self.is_active {
            self.is_active = false;
            if let Some(jit) = &mut self.jit {
                jit.reset(world, self.options.io_only)
            }
        }

        if self.options.optimize {
            let (first_pos, second_pos) = bounds;
            let start_pos = first_pos.min(second_pos);
            let end_pos = first_pos.max(second_pos);
            for y in start_pos.y..=end_pos.y {
                for z in start_pos.z..=end_pos.z {
                    for x in start_pos.x..=end_pos.x {
                        let pos = BlockPos::new(x, y, z);
                        let block = world.get_block(pos);
                        if matches!(block, Block::RedstoneWire { .. }) {
                            block.update(world, pos);
                        }
                    }
                }
            }
        }
        self.options = Default::default();
    }

    fn backend(&mut self) -> &mut BackendDispatcher {
        assert!(
            self.is_active,
            "tried to get redpiler backend when inactive"
        );
        if let Some(jit) = &mut self.jit {
            jit
        } else {
            panic!("redpiler is active but is missing jit backend");
        }
    }

    pub fn tick(&mut self) {
        self.backend().tick();
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
        if let Some(backend) = &mut self.jit {
            backend.inspect(pos);
        } else {
            debug!("cannot inspect when backend is not running");
        }
    }
}

pub struct CompilerInput<'w, W: World> {
    pub world: &'w W,
    pub bounds: (BlockPos, BlockPos),
}
