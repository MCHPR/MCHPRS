use mchprs_blocks::block_entities::BlockEntity;
use mchprs_blocks::blocks::Block;
use mchprs_blocks::BlockPos;
use mchprs_redpiler::{BackendVariant, Compiler, CompilerOptions};
use mchprs_world::storage::Chunk;
use mchprs_world::{TickEntry, TickPriority, World};

#[derive(Clone)]
pub struct TestWorld {
    chunks: Vec<Chunk>,
    to_be_ticked: Vec<TickEntry>,
    size: i32,
}

impl TestWorld {
    pub fn new(size: i32) -> TestWorld {
        let mut chunks = Vec::new();
        for x in 0..size {
            for z in 0..size {
                chunks.push(Chunk::empty(x, z, size as usize));
            }
        }
        TestWorld {
            chunks,
            to_be_ticked: Vec::new(),
            size,
        }
    }

    fn get_chunk_index_for_chunk(&self, chunk_x: i32, chunk_z: i32) -> usize {
        (chunk_x * self.size + chunk_z).unsigned_abs() as usize
    }

    fn get_chunk_index_for_block(&self, block_x: i32, block_z: i32) -> Option<usize> {
        let chunk_x = block_x >> 4;
        let chunk_z = block_z >> 4;
        if chunk_x >= self.size || chunk_z >= self.size || chunk_x < 0 || chunk_z < 0 {
            return None;
        }
        Some(((chunk_x * self.size) + chunk_z).unsigned_abs() as usize)
    }
}

impl World for TestWorld {
    /// Returns the block state id of the block at `pos`
    fn get_block_raw(&self, pos: BlockPos) -> u32 {
        let chunk_index = match self.get_chunk_index_for_block(pos.x, pos.z) {
            Some(idx) => idx,
            None => return 0,
        };
        let chunk = &self.chunks[chunk_index];
        chunk.get_block((pos.x & 0xF) as u32, pos.y as u32, (pos.z & 0xF) as u32)
    }

    /// Sets a block in storage. Returns true if a block was changed.
    fn set_block_raw(&mut self, pos: BlockPos, block: u32) -> bool {
        let chunk_index = match self.get_chunk_index_for_block(pos.x, pos.z) {
            Some(idx) => idx,
            None => return false,
        };

        // Check to see if block is within height limit
        if pos.y >= self.size * 16 || pos.y < 0 {
            return false;
        }

        let chunk = &mut self.chunks[chunk_index];
        chunk.set_block(
            (pos.x & 0xF) as u32,
            pos.y as u32,
            (pos.z & 0xF) as u32,
            block,
        )
    }

    fn delete_block_entity(&mut self, pos: BlockPos) {
        let chunk_index = match self.get_chunk_index_for_block(pos.x, pos.z) {
            Some(idx) => idx,
            None => return,
        };
        let chunk = &mut self.chunks[chunk_index];
        chunk.delete_block_entity(BlockPos::new(pos.x & 0xF, pos.y, pos.z & 0xF));
    }

    fn get_block_entity(&self, pos: BlockPos) -> Option<&BlockEntity> {
        let chunk_index = match self.get_chunk_index_for_block(pos.x, pos.z) {
            Some(idx) => idx,
            None => return None,
        };
        let chunk = &self.chunks[chunk_index];
        chunk.get_block_entity(BlockPos::new(pos.x & 0xF, pos.y, pos.z & 0xF))
    }

    fn set_block_entity(&mut self, pos: BlockPos, block_entity: BlockEntity) {
        let chunk_index = match self.get_chunk_index_for_block(pos.x, pos.z) {
            Some(idx) => idx,
            None => return,
        };
        let chunk = &mut self.chunks[chunk_index];
        chunk.set_block_entity(BlockPos::new(pos.x & 0xF, pos.y, pos.z & 0xF), block_entity);
    }

    fn get_chunk(&self, x: i32, z: i32) -> Option<&Chunk> {
        self.chunks.get(self.get_chunk_index_for_chunk(x, z))
    }

    fn get_chunk_mut(&mut self, x: i32, z: i32) -> Option<&mut Chunk> {
        let chunk_idx = self.get_chunk_index_for_chunk(x, z);
        self.chunks.get_mut(chunk_idx)
    }

    fn schedule_tick(&mut self, pos: BlockPos, delay: u32, priority: TickPriority) {
        self.to_be_ticked.push(TickEntry {
            pos,
            ticks_left: delay,
            tick_priority: priority,
        });
    }

    fn pending_tick_at(&mut self, pos: BlockPos) -> bool {
        self.to_be_ticked.iter().any(|e| e.pos == pos)
    }
}

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
        let max = world.size * 16 - 1;
        let bounds = (BlockPos::new(0, 0, 0), BlockPos::new(max, max, max));
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
    Some(match block {
        Block::RedstoneComparator { comparator } => comparator.powered,
        Block::RedstoneTorch { lit } => lit,
        Block::RedstoneWallTorch { lit, .. } => lit,
        Block::RedstoneRepeater { repeater } => repeater.powered,
        Block::Lever { lever } => lever.powered,
        Block::StoneButton { button } => button.powered,
        Block::StonePressurePlate { powered } => powered,
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
            fn [< $name _rp_direct >]() { $name(TestBackend::Redpiler(BackendVariant::Direct)) }
        }
    };
}
pub(crate) use test_all_backends;
