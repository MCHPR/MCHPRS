mod common;
use common::*;

use mchprs_blocks::blocks::{Block, RedstoneObserver};
use mchprs_blocks::{BlockDirection, BlockFacing};
use mchprs_world::World;

test_all_backends!(lever_on_off);
fn lever_on_off(backend: TestBackend) {
    let lever_pos = pos(0, 1, 0);

    let mut world = TestWorld::new(1);
    make_lever(&mut world, lever_pos);

    let mut runner = BackendRunner::new(world, backend);
    runner.check_block_powered(lever_pos, false);

    runner.use_block(lever_pos);
    runner.check_block_powered(lever_pos, true);

    runner.use_block(lever_pos);
    runner.check_block_powered(lever_pos, false);
}

test_all_backends!(trapdoor_on_off);
fn trapdoor_on_off(backend: TestBackend) {
    let lever_pos = pos(0, 1, 0);
    let trapdoor_pos = pos(1, 0, 0);

    let mut world = TestWorld::new(1);
    make_lever(&mut world, lever_pos);
    world.set_block(trapdoor_pos, trapdoor());

    let mut runner = BackendRunner::new(world, backend);
    runner.check_block_powered(trapdoor_pos, false);

    runner.use_block(lever_pos);
    runner.check_block_powered(trapdoor_pos, true);

    runner.use_block(lever_pos);
    runner.check_block_powered(trapdoor_pos, false);
}

test_all_backends!(lamp_on_off);
fn lamp_on_off(backend: TestBackend) {
    let lever_pos = pos(0, 1, 0);
    let lamp_pos = pos(1, 0, 0);

    let mut world = TestWorld::new(1);
    make_lever(&mut world, lever_pos);
    world.set_block(lamp_pos, Block::RedstoneLamp { lit: false });

    let mut runner = BackendRunner::new(world, backend);
    runner.check_block_powered(lamp_pos, false);

    runner.use_block(lever_pos);
    runner.check_block_powered(lamp_pos, true);

    runner.use_block(lever_pos);
    runner.check_powered_for(lamp_pos, true, 2);
    runner.check_block_powered(lamp_pos, false);
}

test_all_backends!(wall_torch_on_off);
fn wall_torch_on_off(backend: TestBackend) {
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

    let mut runner = BackendRunner::new(world, backend);
    runner.check_block_powered(torch_pos, true);

    runner.use_block(lever_pos);
    runner.check_powered_for(torch_pos, true, 1);
    runner.check_block_powered(torch_pos, false);

    runner.use_block(lever_pos);
    runner.check_powered_for(torch_pos, false, 1);
    runner.check_block_powered(torch_pos, true);
}

test_all_backends!(torch_on_off);
fn torch_on_off(backend: TestBackend) {
    let lever_pos = pos(0, 2, 0);
    let torch_pos = pos(2, 2, 0);

    let mut world = TestWorld::new(1);
    make_lever(&mut world, lever_pos);
    make_wire(&mut world, pos(1, 1, 0));
    place_on_block(&mut world, torch_pos, Block::RedstoneTorch { lit: true });

    let mut runner = BackendRunner::new(world, backend);
    runner.check_block_powered(torch_pos, true);

    runner.use_block(lever_pos);
    runner.check_powered_for(torch_pos, true, 1);
    runner.check_block_powered(torch_pos, false);

    runner.use_block(lever_pos);
    runner.check_powered_for(torch_pos, false, 1);
    runner.check_block_powered(torch_pos, true);
}

test_all_backends!(repeater_on_off);
fn repeater_on_off(backend: TestBackend) {
    let lever_pos = pos(0, 2, 0);
    let trapdoor_pos = pos(2, 1, 0);

    for delay in 1..=4 {
        let mut world = TestWorld::new(1);
        make_lever(&mut world, lever_pos);
        make_repeater(&mut world, pos(1, 1, 0), delay as u8, BlockDirection::West);
        world.set_block(trapdoor_pos, trapdoor());

        let mut runner = BackendRunner::new(world, backend);
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

test_all_backends!(wire_barely_reaches);
fn wire_barely_reaches(backend: TestBackend) {
    let lever_pos = pos(0, 1, 0);
    let trapdoor_pos = pos(16, 1, 0);

    let mut world = TestWorld::new(2);
    make_lever(&mut world, lever_pos);
    // 15 wire blocks between lever and trapdoor
    for x in 1..=15 {
        make_wire(&mut world, pos(x, 1, 0));
    }
    world.set_block(trapdoor_pos, trapdoor());

    let mut runner = BackendRunner::new(world, backend);
    runner.check_block_powered(trapdoor_pos, false);
    runner.use_block(lever_pos);
    runner.check_block_powered(trapdoor_pos, true);
    runner.use_block(lever_pos);
    runner.check_block_powered(trapdoor_pos, false);
}

test_all_backends!(wire_no_reach);
fn wire_no_reach(backend: TestBackend) {
    let lever_pos = pos(0, 1, 0);
    let trapdoor_pos = pos(17, 1, 0);

    let mut world = TestWorld::new(2);
    make_lever(&mut world, lever_pos);
    // 16 wire blocks between lever and trapdoor
    for x in 1..=16 {
        make_wire(&mut world, pos(x, 1, 0));
    }
    world.set_block(trapdoor_pos, trapdoor());

    let mut runner = BackendRunner::new(world, backend);
    runner.check_block_powered(trapdoor_pos, false);
    runner.use_block(lever_pos);
    runner.check_block_powered(trapdoor_pos, false);
    runner.use_block(lever_pos);
    runner.check_block_powered(trapdoor_pos, false);
}

test_all_backends!(observer_on_off);
fn observer_on_off(backend: TestBackend) {
    let lever_pos = pos(0, 1, 0);
    let observer_pos = pos(1, 1, 0);
    let output_pos = pos(2, 1, 0);

    let mut world = TestWorld::new(1);
    make_lever(&mut world, lever_pos);
    world.set_block(
        observer_pos,
        Block::RedstoneObserver {
            observer: RedstoneObserver {
                facing: BlockFacing::West,
                powered: false
            }
        }
    );
    make_wire(&mut world, output_pos);

    let mut runner = BackendRunner::new(world, backend);
    runner.check_block_powered(observer_pos, false);
    runner.check_block_powered(output_pos, false);
    runner.use_block(lever_pos);
    runner.check_block_powered(observer_pos, false);
    runner.check_block_powered(output_pos, false);
    runner.tick();
    runner.check_block_powered(observer_pos, true);
    runner.check_block_powered(output_pos, true);
    runner.tick();
    runner.check_block_powered(observer_pos, false);
    runner.check_block_powered(output_pos, false);
}

test_all_backends!(observer_repeater_lock);
fn observer_repeater_lock(backend: TestBackend) {
    let repeater_pos_1 = pos(0, 1, 1);
    let lever_pos_1 = pos(0, 1, 0);
    let repeater_pos_2 = pos(1, 1, 1);
    let lever_pos_2 = pos(2, 1, 1);
    let observer_pos = pos(0, 2, 1);

    let mut world = TestWorld::new(1);
    make_repeater(&mut world, repeater_pos_1, 1, BlockDirection::North);
    make_lever(&mut world, lever_pos_1);
    make_repeater(&mut world, repeater_pos_2, 1, BlockDirection::East);
    make_lever(&mut world, lever_pos_2);
    world.set_block(
        observer_pos,
        Block::RedstoneObserver {
            observer: RedstoneObserver {
                facing: BlockFacing::Down,
                powered: false
            }
        }
    );

    let mut runner = BackendRunner::new(world, backend);
    runner.use_block(lever_pos_2);
    runner.check_block_powered(observer_pos, false);
    runner.tick();
    runner.check_block_powered(observer_pos, false);
    runner.tick();
    runner.check_block_powered(observer_pos, true);
    runner.tick();
    runner.check_block_powered(observer_pos, false);
    runner.use_block(lever_pos_1); // Should not yield an update since the repeater is now locked
    runner.check_powered_for(observer_pos, false, 5);
}
