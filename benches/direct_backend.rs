mod common;
use common::*;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use mchprs_blocks::{
    blocks::{Block, Lever, LeverFace, RedstoneRepeater},
    BlockDirection, BlockPos,
};
use mchprs_redpiler::{BackendVariant, Compiler, CompilerOptions};
use mchprs_world::World;

fn repeater_grid(c: &mut Criterion) {
    const NUM_CHUNKS: i32 = 2;

    let mut world = TestWorld::new(NUM_CHUNKS);

    let sandstone = Block::Sandstone {};
    let floor_lever = Block::Lever {
        lever: Lever {
            face: LeverFace::Floor,
            facing: Default::default(),
            powered: false,
        },
    };
    let lamp = Block::RedstoneLamp { lit: false };
    let repeater_east = Block::RedstoneRepeater {
        repeater: RedstoneRepeater {
            facing: BlockDirection::West,
            ..Default::default()
        },
    };
    let repeater_south = Block::RedstoneRepeater {
        repeater: RedstoneRepeater {
            facing: BlockDirection::North,
            ..Default::default()
        },
    };

    for y in (0..16 * NUM_CHUNKS).step_by(2) {
        // Construct a grid of redstone lamps and repeaters going east and south
        // with a lever at the northwest corner to start the reaction.
        for x in (0..16 * NUM_CHUNKS).step_by(2) {
            for z in (0..16 * NUM_CHUNKS).step_by(2) {
                if (x, z) == (0, 0) {
                    world.set_block(BlockPos::new(x, y, z), sandstone);
                    world.set_block(BlockPos::new(x, y + 1, z), floor_lever);
                } else {
                    world.set_block(BlockPos::new(x, y + 1, z), lamp);
                }
                world.set_block(BlockPos::new(x + 1, y + 1, z), sandstone);
                world.set_block(BlockPos::new(x + 1, y + 1, z), repeater_east);
                world.set_block(BlockPos::new(x, y + 1, z + 1), sandstone);
                world.set_block(BlockPos::new(x, y + 1, z + 1), repeater_south);
            }
        }
    }

    let setup_and_compile = || {
        let bounds = (BlockPos::zero(), BlockPos::splat(16 * NUM_CHUNKS - 1));
        let compile_options = CompilerOptions {
            backend_variant: BackendVariant::Direct,
            ..Default::default()
        };
        let mut compiler = Compiler::default();
        compiler.compile(
            std::hint::black_box(&world),
            bounds,
            compile_options.clone(),
            Default::default(),
            Default::default(),
        );
        for y in (0..16 * NUM_CHUNKS).step_by(2) {
            let pos = BlockPos::new(0, y + 1, 0);
            assert_eq!(world.get_block(pos), floor_lever);
            compiler.on_use_block(pos);
        }
        compiler
    };

    let run_backend = |compiler: &mut Compiler| {
        while compiler.has_pending_ticks() {
            compiler.tick();
        }
    };

    // Do a test run to make sure the redstone setup works as expected and that we are
    // not just benchmarking a broken redstone contraption where nothing happens.
    {
        let mut world = world.clone();
        let mut compiler = setup_and_compile();
        run_backend(&mut compiler);
        compiler.flush(&mut world);
        let x = 16 * NUM_CHUNKS - 2;
        let z = 16 * NUM_CHUNKS - 2;
        for y in (0..16 * NUM_CHUNKS).step_by(2) {
            // All redstone lamps at the south east corner should be lit.
            assert_eq!(
                world.get_block(BlockPos::new(x, y + 1, z)),
                Block::RedstoneLamp { lit: true }
            );
        }
    }

    c.bench_function("repeater_grid", |b| {
        b.iter_batched_ref(setup_and_compile, run_backend, BatchSize::LargeInput);
    });
}

criterion_group!(benches, repeater_grid);
criterion_main!(benches);
