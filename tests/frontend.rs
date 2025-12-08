mod common;
use common::*;

use expect_test::expect_file;
use petgraph::dot::Dot;

use mchprs_blocks::{
    blocks::{Block, ComparatorMode, Lever, LeverFace},
    BlockDirection,
};
use mchprs_redpiler::{CompileGraph, CompilerInput, CompilerOptions, PassManager};
use mchprs_world::{TickPriority, World};

enum OptLevel {
    Unoptimized,
    Optimized,
}

fn test_frontend(world: &TestWorld, expected_dot: expect_test::ExpectFile, opt_level: OptLevel) {
    let compile_options = match opt_level {
        OptLevel::Unoptimized => CompilerOptions {
            optimize: false,
            ..Default::default()
        },
        OptLevel::Optimized => CompilerOptions {
            optimize: true,
            io_only: true,
            ..Default::default()
        },
    };
    let pass_manager = PassManager::default();
    let graph = pass_manager.run_passes(
        &compile_options,
        &CompilerInput {
            world,
            bounds: world.bounds(),
        },
        Default::default(),
    );
    let dot = format!("{:?}", Dot::<&CompileGraph>::new(&graph));
    expected_dot.assert_eq(&dot);
}

const SANDSTONE: Block = Block::Sandstone {};
const REDSTONE_BLOCK: Block = Block::RedstoneBlock {};

const REDSTONE_LAMP_UNLIT: Block = Block::RedstoneLamp { lit: false };
const REDSTONE_LAMP_LIT: Block = Block::RedstoneLamp { lit: true };

const LEVER_GROUND_DEACTIVATED: Block = Block::Lever {
    lever: Lever {
        face: LeverFace::Floor,
        facing: BlockDirection::North,
        powered: false,
    },
};

const TORCH_GROUND_UNLIT: Block = Block::RedstoneTorch { lit: false };
const TORCH_GROUND_LIT: Block = Block::RedstoneTorch { lit: true };

#[test]
fn lever_on_constantly_powered_lamp() {
    let mut world = TestWorld::new(1);
    world.set_block(pos(0, 0, 0), REDSTONE_BLOCK);
    world.set_block(pos(0, 1, 0), REDSTONE_LAMP_LIT);
    world.set_block(pos(0, 2, 0), LEVER_GROUND_DEACTIVATED);

    // Unoptimized
    let expected = expect_file!["test_expects/lever_on_constantly_powered_lamp_UNOPTIMIZED.dot"];
    test_frontend(&world, expected, OptLevel::Unoptimized);

    // Optimized
    // TODO: Missed optimization:
    // The lever has no effect on the lamp.
    let expected = expect_file!["test_expects/lever_on_constantly_powered_lamp_OPTIMIZED.dot"];
    test_frontend(&world, expected, OptLevel::Optimized);
}

#[test]
fn lit_torch_with_no_inputs() {
    let mut world = TestWorld::new(1);
    world.set_block(pos(0, 0, 0), SANDSTONE);
    world.set_block(pos(0, 1, 0), TORCH_GROUND_LIT);
    world.set_block(pos(0, 2, 0), REDSTONE_LAMP_LIT);

    // Unoptimized
    let expected = expect_file!["test_expects/lit_torch_with_no_inputs_UNOPTIMIZED.dot"];
    test_frontend(&world, expected, OptLevel::Unoptimized);

    // Optimized
    let expected = expect_file!["test_expects/lit_torch_with_no_inputs_OPTIMIZED.dot"];
    test_frontend(&world, expected, OptLevel::Optimized);
}

#[test]
fn unlit_torch_on_redstone_block() {
    let mut world = TestWorld::new(1);
    world.set_block(pos(0, 0, 0), REDSTONE_BLOCK);
    world.set_block(pos(0, 1, 0), TORCH_GROUND_UNLIT);
    world.set_block(pos(0, 2, 0), REDSTONE_LAMP_UNLIT);

    // Unoptimized
    let expected = expect_file!["test_expects/unlit_torch_on_redstone_block_UNOPTIMIZED.dot"];
    test_frontend(&world, expected, OptLevel::Unoptimized);

    // Optimized
    let expected = expect_file!["test_expects/unlit_torch_on_redstone_block_OPTIMIZED.dot"];
    test_frontend(&world, expected, OptLevel::Optimized);
}

#[test]
fn lever_with_many_edges_to_same_lamp() {
    let mut world = TestWorld::new(1);
    world.set_block(pos(1, 1, 1), REDSTONE_LAMP_UNLIT);
    world.set_block(pos(1, 2, 1), LEVER_GROUND_DEACTIVATED);
    make_wire(&mut world, pos(0, 1, 1));
    make_wire(&mut world, pos(1, 1, 0));
    make_wire(&mut world, pos(2, 1, 1));
    make_wire(&mut world, pos(1, 1, 2));

    // Unoptimized
    let expected = expect_file!["test_expects/lever_with_many_edges_to_same_lamp_UNOPTIMIZED.dot"];
    test_frontend(&world, expected, OptLevel::Unoptimized);

    // Optimized
    let expected = expect_file!["test_expects/lever_with_many_edges_to_same_lamp_OPTIMIZED.dot"];
    test_frontend(&world, expected, OptLevel::Optimized);
}

#[test]
fn long_redstone_wire() {
    let mut world = TestWorld::new(2);
    make_lever(&mut world, pos(0, 1, 0));
    world.set_block(pos(0, 1, 20), REDSTONE_LAMP_UNLIT);
    for z in 1..20 {
        make_wire(&mut world, pos(0, 1, z));
    }

    // Unoptimized
    let expected = expect_file!["test_expects/long_redstone_wire_UNOPTIMIZED.dot"];
    test_frontend(&world, expected, OptLevel::Unoptimized);

    // Optimized
    let expected = expect_file!["test_expects/long_redstone_wire_OPTIMIZED.dot"];
    test_frontend(&world, expected, OptLevel::Optimized);
}

#[test]
fn long_redstone_wire_with_comparator() {
    let mut world = TestWorld::new(1);
    make_lever(&mut world, pos(0, 1, 0));
    for z in 1..10 {
        make_wire(&mut world, pos(0, 1, z));
    }
    make_comparator(
        &mut world,
        pos(0, 1, 10),
        ComparatorMode::Compare,
        BlockDirection::North,
    );
    for z in 11..20 {
        make_wire(&mut world, pos(0, 1, z));
    }
    world.set_block(pos(0, 1, 20), REDSTONE_LAMP_UNLIT);

    // Unoptimized
    // FIXME: Unoptimized mode should not be removing edges.
    let expected = expect_file!["test_expects/long_redstone_wire_with_comparator_UNOPTIMIZED.dot"];
    test_frontend(&world, expected, OptLevel::Unoptimized);

    // Optimized
    let expected = expect_file!["test_expects/long_redstone_wire_with_comparator_OPTIMIZED.dot"];
    test_frontend(&world, expected, OptLevel::Optimized);
}

#[test]
fn unpowered_repeater_clock() {
    let mut world = TestWorld::new(1);
    make_wire(&mut world, pos(0, 1, 0));
    make_repeater(&mut world, pos(0, 1, 1), 1, BlockDirection::East);
    world.set_block(pos(0, 1, 2), REDSTONE_LAMP_UNLIT);
    make_wire(&mut world, pos(1, 1, 2));
    make_repeater(&mut world, pos(1, 1, 1), 1, BlockDirection::West);
    world.set_block(pos(1, 1, 0), REDSTONE_LAMP_UNLIT);

    // Unoptimized
    let expected = expect_file!["test_expects/unpowered_repeater_clock_UNOPTIMIZED.dot"];
    test_frontend(&world, expected, OptLevel::Unoptimized);

    // Optimized
    let expected = expect_file!["test_expects/unpowered_repeater_clock_OPTIMIZED.dot"];
    test_frontend(&world, expected, OptLevel::Optimized);
}

#[test]
fn repeater_clock_duplicated_repeaters() {
    //  L  W  W  W  W  W  W
    //    RN RN RN RS RS RS
    //  L  W  W  W  W  W  W
    let mut world = TestWorld::new(1);
    make_lever(&mut world, pos(0, 1, 0));
    world.set_block(pos(0, 0, 2), REDSTONE_LAMP_UNLIT);
    for x in 1..7 {
        let repeater_direction = match x {
            ..4 => BlockDirection::North,
            _ => BlockDirection::South,
        };
        make_wire(&mut world, pos(x, 1, 0));
        make_repeater(&mut world, pos(x, 1, 1), 1, repeater_direction);
        make_wire(&mut world, pos(x, 1, 2));
    }

    // Unoptimized
    let expected = expect_file!["test_expects/repeater_clock_duplicated_repeaters_UNOPTIMIZED.dot"];
    test_frontend(&world, expected, OptLevel::Unoptimized);

    // Optimized
    // TODO: Missed optimization:
    // A lot of redundant edges.
    let expected = expect_file!["test_expects/repeater_clock_duplicated_repeaters_OPTIMIZED.dot"];
    test_frontend(&world, expected, OptLevel::Optimized);
}

#[test]
fn compare_mode_full_ss_rear_input() {
    let mut world = TestWorld::new(1);
    world.set_block(pos(0, 1, 0), REDSTONE_LAMP_UNLIT);
    make_comparator(
        &mut world,
        pos(0, 1, 1),
        ComparatorMode::Compare,
        BlockDirection::South,
    );
    make_wire(&mut world, pos(0, 1, 2));
    make_lever(&mut world, pos(0, 1, 3)); // rear input
    make_repeater(&mut world, pos(1, 1, 1), 1, BlockDirection::East);
    make_lever(&mut world, pos(2, 1, 1)); // side input

    // Unoptimized
    let expected = expect_file!["test_expects/compare_mode_full_ss_rear_input_UNOPTIMIZED.dot"];
    test_frontend(&world, expected, OptLevel::Unoptimized);

    // Optimized
    // TODO: Missed optimization:
    // The side input of the comparator has no effect on the comparator.
    let expected = expect_file!["test_expects/compare_mode_full_ss_rear_input_OPTIMIZED.dot"];
    test_frontend(&world, expected, OptLevel::Optimized);
}

#[test]
fn repeater_chain_with_pending_update() {
    let mut world = TestWorld::new(1);
    world.set_block(pos(0, 1, 0), REDSTONE_BLOCK);
    make_repeater(&mut world, pos(1, 1, 0), 1, BlockDirection::West);
    make_repeater(&mut world, pos(2, 1, 0), 1, BlockDirection::West);
    world.set_block(pos(3, 1, 0), REDSTONE_LAMP_UNLIT);
    // schedule an update for the first repeater
    world.schedule_tick(pos(1, 1, 0), 1, TickPriority::Normal);

    // Unoptimized
    let expected = expect_file!["test_expects/repeater_chain_with_pending_update_UNOPTIMIZED.dot"];
    test_frontend(&world, expected, OptLevel::Unoptimized);

    // Optimized
    // FIXME: Illegal optimization:
    // The repeaters must not be optimized away because they become powered in the next ticks.
    let expected = expect_file!["test_expects/repeater_chain_with_pending_update_OPTIMIZED.dot"];
    test_frontend(&world, expected, OptLevel::Optimized);
}

#[test]
fn multiple_different_constants() {
    let mut world = TestWorld::new(1);
    for x in 0..4 {
        make_wire(&mut world, pos(x, 1, 1));
    }
    world.set_block(pos(0, 1, 0), REDSTONE_BLOCK);
    world.set_block(pos(2, 1, 0), REDSTONE_BLOCK);
    world.set_block(pos(4, 1, 0), REDSTONE_LAMP_UNLIT);
    make_comparator(
        &mut world,
        pos(4, 1, 1),
        ComparatorMode::Subtract,
        BlockDirection::South,
    );
    make_lever(&mut world, pos(4, 1, 2));

    // Unoptimized
    let expected = expect_file!["test_expects/multiple_different_constants_UNOPTIMIZED.dot"];
    test_frontend(&world, expected, OptLevel::Unoptimized);

    // Optimized
    // TODO: Missed optimization:
    // Redundant edge from constant to comparator.
    let expected = expect_file!["test_expects/multiple_different_constants_OPTIMIZED.dot"];
    test_frontend(&world, expected, OptLevel::Optimized);
}
