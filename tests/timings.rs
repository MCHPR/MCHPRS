mod common;
use common::*;
use mchprs_blocks::BlockDirection;

test_all_backends!(repeater_t_flip_flop);
fn repeater_t_flip_flop(backend: TestBackend) {
    // RS -> Repeater South
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
