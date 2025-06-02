mod common;
use common::*;

use mchprs_blocks::blocks::{Block, ComparatorMode, RedstoneObserver};
use mchprs_blocks::{BlockDirection, BlockFace, BlockFacing};
use mchprs_world::World;

test_all_backends!(repeater_t_flip_flop);
fn repeater_t_flip_flop(backend: TestBackend) {
    // RN -> Repeater North
    // Layout:
    // W RN W
    // W RN RE
    // L

    let mut world = TestWorld::new(1);

    let output_pos = pos(1, 1, 2);
    let lever_pos = pos(0, 1, 0);

    make_lever(&mut world, lever_pos);
    make_wire(&mut world, pos(1, 1, 0));
    make_wire(&mut world, pos(2, 1, 0));

    make_repeater(&mut world, pos(1, 1, 1), 1, BlockDirection::North);
    make_repeater(&mut world, pos(2, 1, 1), 1, BlockDirection::North);

    make_repeater(&mut world, output_pos, 1, BlockDirection::East);
    make_wire(&mut world, pos(2, 1, 2));

    let mut runner = BackendRunner::new(world, backend);
    // Set up initial state
    runner.use_block(lever_pos);
    runner.check_powered_for(output_pos, false, 2);

    // Toggle flip flop on
    runner.use_block(lever_pos);
    runner.check_powered_for(output_pos, false, 2);
    runner.use_block(lever_pos);
    runner.check_powered_for(output_pos, true, 10);

    // Toggle flip flop off
    runner.use_block(lever_pos);
    runner.check_powered_for(output_pos, true, 2);
    runner.use_block(lever_pos);
    runner.check_powered_for(output_pos, false, 10);
}

test_all_backends!(pulse_gen_2t);
fn pulse_gen_2t(backend: TestBackend) {
    let output_pos = pos(4, 1, 1);
    let lever_pos = pos(0, 1, 1);

    let mut world = TestWorld::new(1);

    make_wire(&mut world, pos(1, 1, 0));
    make_repeater(&mut world, pos(2, 1, 0), 2, BlockDirection::West);
    make_wire(&mut world, pos(3, 1, 0));

    make_lever(&mut world, lever_pos);
    make_wire(&mut world, pos(1, 1, 1));
    make_wire(&mut world, pos(2, 1, 1));
    make_comparator(
        &mut world,
        pos(3, 1, 1),
        ComparatorMode::Subtract,
        BlockDirection::West,
    );
    place_on_block(&mut world, output_pos, trapdoor());

    let mut runner = BackendRunner::new(world, backend);

    runner.use_block(lever_pos);
    runner.check_powered_for(output_pos, false, 1);
    runner.check_powered_for(output_pos, true, 2);
    runner.check_powered_for(output_pos, false, 10);
}

test_all_backends!(pulse_gen_1t);
fn pulse_gen_1t(backend: TestBackend) {
    let output_pos = pos(5, 1, 1);
    let lever_pos = pos(0, 1, 1);

    let mut world = TestWorld::new(1);

    make_wire(&mut world, pos(1, 1, 0));
    make_repeater(&mut world, pos(2, 1, 0), 2, BlockDirection::West);
    make_wire(&mut world, pos(3, 1, 0));
    make_wire(&mut world, pos(4, 1, 0));

    make_lever(&mut world, lever_pos);
    make_wire(&mut world, pos(1, 1, 1));
    make_wire(&mut world, pos(2, 1, 1));
    make_comparator(
        &mut world,
        pos(3, 1, 1),
        ComparatorMode::Subtract,
        BlockDirection::West,
    );
    place_on_block(&mut world, pos(4, 1, 1), Block::Sandstone {});
    place_on_block(&mut world, output_pos, trapdoor());

    let mut runner = BackendRunner::new(world, backend);

    runner.use_block(lever_pos);
    runner.check_powered_for(output_pos, false, 1);
    runner.check_powered_for(output_pos, true, 1);
    runner.check_powered_for(output_pos, false, 10);
}

test_all_backends!(observer_clock);
fn observer_clock(backend: TestBackend) {
    let observer_pos_1 = pos(0, 0, 0);
    let observer_pos_2 = pos(0, 1, 0);

    let mut world = TestWorld::new(1);
    world.set_block(
        observer_pos_1,
        Block::RedstoneObserver {
            observer: RedstoneObserver {
                facing: BlockFacing::Up,
                powered: false
            }
        }
    );
    world.set_block(
        observer_pos_2,
        Block::RedstoneObserver {
            observer: RedstoneObserver {
                facing: BlockFacing::Down,
                powered: false
            }
        }
    );

    let mut runner = BackendRunner::new(world, backend);
    runner.trigger_observer(observer_pos_1, BlockFace::Top);
    for _ in 0..10 {
        // The vanilla observer clock sequence includes a phase where both observers are depowered
        // (This is because of their update order)
        runner.check_block_powered(observer_pos_1, false);
        runner.check_block_powered(observer_pos_2, false);
        runner.tick();
        runner.check_block_powered(observer_pos_1, true);
        runner.check_block_powered(observer_pos_2, false);
        runner.tick();
        runner.check_block_powered(observer_pos_1, false);
        runner.check_block_powered(observer_pos_2, true);
        runner.tick();
    }
}
