mod common;

use common::{AllBackendRunner, TestWorld};
use mchprs_blocks::blocks::{Block, Lever, LeverFace};
use mchprs_blocks::{BlockDirection, BlockPos};
use mchprs_redstone::wire::make_cross;
use mchprs_world::World;

fn pos(x: i32, y: i32, z: i32) -> BlockPos {
    BlockPos::new(x, y, z)
}

/// Creates a lever at `lever_pos` with a block of sandstone below it
fn make_lever(world: &mut TestWorld, lever_pos: BlockPos) {
    world.set_block(lever_pos - pos(0, 1, 0), Block::Sandstone {});
    world.set_block(
        lever_pos,
        Block::Lever {
            lever: Lever {
                face: LeverFace::Floor,
                ..Default::default()
            },
        },
    );
}

#[test]
fn lever_on_off() {
    let lever_pos = pos(0, 1, 0);

    let mut world = TestWorld::new(1);
    make_lever(&mut world, lever_pos);

    let mut runner = AllBackendRunner::new(world);
    runner.check_block_powered(lever_pos, false);

    runner.use_block(lever_pos);
    runner.check_block_powered(lever_pos, true);

    runner.use_block(lever_pos);
    runner.check_block_powered(lever_pos, false);
}

#[test]
fn trapdoor_on_off() {
    let lever_pos = pos(0, 1, 0);
    let trapdoor_pos = pos(1, 0, 0);

    let mut world = TestWorld::new(1);
    make_lever(&mut world, lever_pos);
    world.set_block(
        trapdoor_pos,
        Block::IronTrapdoor {
            facing: Default::default(),
            half: Default::default(),
            powered: false,
        },
    );

    let mut runner = AllBackendRunner::new(world);
    runner.check_block_powered(trapdoor_pos, false);

    runner.use_block(lever_pos);
    runner.check_block_powered(trapdoor_pos, true);

    runner.use_block(lever_pos);
    runner.check_block_powered(trapdoor_pos, false);
}

#[test]
fn lamp_on_off() {
    let lever_pos = pos(0, 1, 0);
    let lamp_pos = pos(1, 0, 0);

    let mut world = TestWorld::new(1);
    make_lever(&mut world, lever_pos);
    world.set_block(lamp_pos, Block::RedstoneLamp { lit: false });

    let mut runner = AllBackendRunner::new(world);
    runner.check_block_powered(lamp_pos, false);

    runner.use_block(lever_pos);
    runner.check_block_powered(lamp_pos, true);

    runner.use_block(lever_pos);
    runner.check_powered_for(lamp_pos, true, 2);
    runner.check_block_powered(lamp_pos, false);
}

#[test]
fn wall_torch_on_off() {
    let lever_pos = pos(0, 1, 0);
    let torch_pos = pos(1, 0, 0);

    let mut world = TestWorld::new(1);
    make_lever(&mut world, lever_pos);
    world.set_block(
        torch_pos,
        Block::RedstoneWallTorch {
            lit: true,
            facing: BlockDirection::East,
        },
    );

    let mut runner = AllBackendRunner::new(world);
    runner.check_block_powered(torch_pos, true);

    runner.use_block(lever_pos);
    runner.check_powered_for(torch_pos, true, 1);
    runner.check_block_powered(torch_pos, false);

    runner.use_block(lever_pos);
    runner.check_powered_for(torch_pos, false, 1);
    runner.check_block_powered(torch_pos, true);
}

#[test]
fn torch_on_off() {
    let lever_pos = pos(0, 2, 0);
    let torch_pos = pos(2, 2, 0);

    let mut world = TestWorld::new(1);
    make_lever(&mut world, lever_pos);
    world.set_block(pos(1, 0, 0), Block::Sandstone {});
    world.set_block(
        pos(1, 1, 0),
        Block::RedstoneWire {
            wire: make_cross(0),
        },
    );
    world.set_block(pos(2, 1, 0), Block::Sandstone {});
    world.set_block(torch_pos, Block::RedstoneTorch { lit: true });

    let mut runner = AllBackendRunner::new(world);
    runner.check_block_powered(torch_pos, true);

    runner.use_block(lever_pos);
    runner.check_powered_for(torch_pos, true, 1);
    runner.check_block_powered(torch_pos, false);

    runner.use_block(lever_pos);
    runner.check_powered_for(torch_pos, false, 1);
    runner.check_block_powered(torch_pos, true);
}
