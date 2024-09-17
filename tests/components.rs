mod common;

use common::{AllBackendRunner, TestWorld};
use mchprs_blocks::blocks::{Block, Lever, LeverFace, RedstoneRepeater};
use mchprs_blocks::{BlockDirection, BlockPos};
use mchprs_redstone::wire::make_cross;
use mchprs_world::World;

fn pos(x: i32, y: i32, z: i32) -> BlockPos {
    BlockPos::new(x, y, z)
}

fn place_on_block(world: &mut TestWorld, block_pos: BlockPos, block: Block) {
    world.set_block(block_pos - pos(0, 1, 0), Block::Sandstone {});
    world.set_block(block_pos, block);
}

fn trapdoor() -> Block {
    Block::IronTrapdoor {
        facing: Default::default(),
        half: Default::default(),
        powered: false,
    }
}

/// Creates a lever at `lever_pos` with a block of sandstone below it
fn make_lever(world: &mut TestWorld, lever_pos: BlockPos) {
    place_on_block(
        world,
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
    world.set_block(trapdoor_pos, trapdoor());

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
    place_on_block(
        &mut world,
        pos(1, 1, 0),
        Block::RedstoneWire {
            wire: make_cross(0),
        },
    );
    place_on_block(&mut world, torch_pos, Block::RedstoneTorch { lit: true });

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
fn repeater_on_off() {
    let lever_pos = pos(0, 2, 0);
    let trapdoor_pos = pos(2, 1, 0);

    for delay in 1..=4 {
        let mut world = TestWorld::new(1);
        make_lever(&mut world, lever_pos);
        place_on_block(
            &mut world,
            pos(1, 1, 0),
            Block::RedstoneRepeater {
                repeater: RedstoneRepeater {
                    facing: BlockDirection::West,
                    delay: delay as u8,
                    ..Default::default()
                },
            },
        );
        world.set_block(trapdoor_pos, trapdoor());

        let mut runner = AllBackendRunner::new(world);
        runner.check_block_powered(trapdoor_pos, false);

        // Check with a 1 tick pulse
        runner.use_block(lever_pos);
        runner.check_powered_for(trapdoor_pos, false, delay);
        runner.check_block_powered(trapdoor_pos, true);
        runner.use_block(lever_pos);
        runner.check_powered_for(trapdoor_pos, true, delay);
        runner.check_block_powered(trapdoor_pos, false);

        // Now a 0 tick pulse
        runner.use_block(lever_pos);
        runner.use_block(lever_pos);
        runner.check_powered_for(trapdoor_pos, false, delay);
        runner.check_powered_for(trapdoor_pos, true, delay);
        runner.check_block_powered(trapdoor_pos, false);
    }
}
