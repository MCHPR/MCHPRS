use mchprs_blocks::blocks::{Block, Comparator, ComparatorMode, LeverFace, Repeater};
use mchprs_blocks::{BlockDirection, BlockPos};
use mchprs_redpiler::{BackendVariant, Compiler, CompilerOptions};
use mchprs_redstone::wire::make_cross;
use mchprs_world::testing::TestWorld;
use mchprs_world::World;

struct RedpilerInstance {
    options: CompilerOptions,
    compiler: Compiler,
}

impl RedpilerInstance {
    fn new(world: &TestWorld, variant: BackendVariant) -> RedpilerInstance {
        let options = CompilerOptions {
            backend_variant: variant,
            ..Default::default()
        };
        let mut compiler = Compiler::default();
        let max_x = world.x_size * 16 - 1;
        let max_y = world.y_size * 16 - 1;
        let max_z = world.z_size * 16 - 1;
        let bounds = (BlockPos::new(0, 0, 0), BlockPos::new(max_x, max_y, max_z));
        let monitor = Default::default();
        let ticks = world.to_be_ticked.clone();
        compiler.compile(world, bounds, options.clone(), ticks, monitor);
        RedpilerInstance { options, compiler }
    }
}

#[derive(Copy, Clone)]
pub enum TestBackend {
    Redstone,
    Redpiler(BackendVariant),
}

pub struct BackendRunner {
    world: TestWorld,
    redpiler: Option<RedpilerInstance>,
}

impl BackendRunner {
    pub fn new(world: TestWorld, backend: TestBackend) -> BackendRunner {
        match backend {
            TestBackend::Redstone => BackendRunner {
                world,
                redpiler: None,
            },
            TestBackend::Redpiler(variant) => BackendRunner {
                redpiler: Some(RedpilerInstance::new(&world, variant)),
                world,
            },
        }
    }

    pub fn tick(&mut self) {
        if let Some(redpiler) = &mut self.redpiler {
            redpiler.compiler.tick();
            redpiler.compiler.flush(&mut self.world);
            return;
        }

        self.world
            .to_be_ticked
            .sort_by_key(|e| (e.ticks_left, e.tick_priority));
        for pending in &mut self.world.to_be_ticked {
            pending.ticks_left = pending.ticks_left.saturating_sub(1);
        }
        while self.world.to_be_ticked.first().map_or(1, |e| e.ticks_left) == 0 {
            let entry = self.world.to_be_ticked.remove(0);
            mchprs_redstone::tick(self.world.get_block(entry.pos), &mut self.world, entry.pos);
        }
    }

    pub fn use_block(&mut self, pos: BlockPos) {
        if let Some(redpiler) = &mut self.redpiler {
            redpiler.compiler.on_use_block(pos);
            redpiler.compiler.flush(&mut self.world);
            return;
        }
        mchprs_redstone::on_use(self.world.get_block(pos), &mut self.world, pos);
    }

    pub fn check_block_powered(&self, pos: BlockPos, powered: bool) {
        if let Some(redpiler) = &self.redpiler {
            assert_eq!(
                is_block_powered(self.world.get_block(pos)),
                Some(powered),
                "when testing on redpiler options: {:#?}",
                redpiler.options
            );
            return;
        }
        assert_eq!(
            is_block_powered(self.world.get_block(pos)),
            Some(powered),
            "when testing with the base redstone implementation"
        );
    }

    pub fn check_powered_for(&mut self, pos: BlockPos, powered: bool, ticks: usize) {
        for _ in 0..ticks {
            self.check_block_powered(pos, powered);
            self.tick();
        }
    }
}

fn is_block_powered(block: Block) -> Option<bool> {
    if let Some(powered) = block.clone().get_pressure_plate_powered() {
        return Some(*powered);
    }
    Some(match block {
        Block::Comparator(comparator) => comparator.powered,
        Block::RedstoneTorch { lit } => lit,
        Block::RedstoneWallTorch { lit, .. } => lit,
        Block::Repeater(repeater) => repeater.powered,
        Block::Lever { powered, .. } => powered,
        Block::StoneButton { powered, .. } => powered,
        Block::RedstoneLamp { lit } => lit,
        Block::IronTrapdoor { powered, .. } => powered,
        Block::NoteBlock { powered, .. } => powered,
        _ => return None,
    })
}

macro_rules! test_all_backends {
    ($name:ident) => {
        paste::paste! {
            #[test]
            fn [< $name _redstone >]() { $name(TestBackend::Redstone) }
            #[test]
            fn [< $name _rp_direct >]() { $name(TestBackend::Redpiler(::mchprs_redpiler::BackendVariant::Direct)) }
        }
    };
}
pub(crate) use test_all_backends;

/// Helper function to create a BlockPos
pub fn pos(x: i32, y: i32, z: i32) -> BlockPos {
    BlockPos::new(x, y, z)
}

/// Place a block with a block of sandstone below it
pub fn place_on_block(world: &mut TestWorld, block_pos: BlockPos, block: Block) {
    world.set_block(block_pos - pos(0, 1, 0), Block::Sandstone {});
    world.set_block(block_pos, block);
}

pub fn trapdoor() -> Block {
    Block::IronTrapdoor {
        facing: Default::default(),
        half: Default::default(),
        powered: false,
        open: false,
        waterlogged: false,
    }
}

/// Creates a lever at `lever_pos` with a block of sandstone below it
pub fn make_lever(world: &mut TestWorld, lever_pos: BlockPos) {
    place_on_block(
        world,
        lever_pos,
        Block::Lever {
            face: LeverFace::Floor,
            facing: BlockDirection::West,
            powered: false,
        },
    );
}

/// Creates a repeater at `repeater_pos` with a block of sandstone below it
pub fn make_repeater(
    world: &mut TestWorld,
    repeater_pos: BlockPos,
    delay: u8,
    direction: BlockDirection,
) {
    place_on_block(
        world,
        repeater_pos,
        Block::Repeater(Repeater {
            delay,
            facing: direction,
            ..Default::default()
        }),
    );
}

/// Creates a wire at `wire_pos` with a block of sandstone below it
pub fn make_wire(world: &mut TestWorld, wire_pos: BlockPos) {
    place_on_block(world, wire_pos, Block::RedstoneWire(make_cross(0)));
}

/// Creates a comparator at `comp_pos` with a block of sandstone below it
pub fn make_comparator(
    world: &mut TestWorld,
    comp_pos: BlockPos,
    mode: ComparatorMode,
    facing: BlockDirection,
) {
    place_on_block(
        world,
        comp_pos,
        Block::Comparator(Comparator {
            mode,
            facing,
            ..Default::default()
        }),
    );
}
