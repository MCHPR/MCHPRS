mod common;
use common::*;

use expect_test::expect;
use petgraph::dot::Dot;

use mchprs_blocks::{
    blocks::{Block, ComparatorMode, Lever, LeverFace},
    BlockDirection,
};
use mchprs_redpiler::{CompileGraph, CompilerInput, CompilerOptions, PassManager};
use mchprs_world::World;

enum OptLevel {
    Unoptimized,
    Optimized,
}

fn test_frontend(world: &TestWorld, expected_dot: expect_test::Expect, opt_level: OptLevel) {
    let compile_options = CompilerOptions {
        optimize: match opt_level {
            OptLevel::Unoptimized => false,
            OptLevel::Optimized => true,
        },
        ..Default::default()
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
fn lever_on_lit_redstone_lamp_with_constant_input() {
    let mut world = TestWorld::new(1);
    world.set_block(pos(0, 0, 0), REDSTONE_BLOCK);
    world.set_block(pos(0, 1, 0), REDSTONE_LAMP_LIT);
    world.set_block(pos(0, 2, 0), LEVER_GROUND_DEACTIVATED);

    // Unoptimized
    let expected = expect![[r#"
        digraph {
            0 [ label = "CompileNode { ty: Constant, block: Some((BlockPos { x: 0, y: 0, z: 0 }, 9223)), state: NodeState { powered: false, repeater_locked: false, output_strength: 15 }, is_input: false, is_output: false, annotations: Annotations }" ]
            1 [ label = "CompileNode { ty: Lamp, block: Some((BlockPos { x: 0, y: 1, z: 0 }, 7417)), state: NodeState { powered: true, repeater_locked: false, output_strength: 15 }, is_input: false, is_output: true, annotations: Annotations }" ]
            2 [ label = "CompileNode { ty: Lever, block: Some((BlockPos { x: 0, y: 2, z: 0 }, 5627)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: true, is_output: false, annotations: Annotations }" ]
            2 -> 1 [ label = "CompileLink { ty: Default, ss: 0 }" ]
            0 -> 1 [ label = "CompileLink { ty: Default, ss: 0 }" ]
        }
    "#]];
    test_frontend(&world, expected, OptLevel::Unoptimized);

    // Optimized
    // TODO: Missed optimization:
    // The lever has no effect on the lamp.
    let expected = expect![[r#"
        digraph {
            1 [ label = "CompileNode { ty: Lamp, block: Some((BlockPos { x: 0, y: 1, z: 0 }, 7417)), state: NodeState { powered: true, repeater_locked: false, output_strength: 15 }, is_input: false, is_output: true, annotations: Annotations }" ]
            2 [ label = "CompileNode { ty: Lever, block: Some((BlockPos { x: 0, y: 2, z: 0 }, 5627)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: true, is_output: false, annotations: Annotations }" ]
            3 [ label = "CompileNode { ty: Constant, block: None, state: NodeState { powered: false, repeater_locked: false, output_strength: 15 }, is_input: false, is_output: false, annotations: Annotations }" ]
            2 -> 1 [ label = "CompileLink { ty: Default, ss: 0 }" ]
            3 -> 1 [ label = "CompileLink { ty: Default, ss: 0 }" ]
        }
    "#]];
    test_frontend(&world, expected, OptLevel::Optimized);
}

#[test]
fn lit_torch_with_no_inputs() {
    let mut world = TestWorld::new(1);
    world.set_block(pos(0, 0, 0), SANDSTONE);
    world.set_block(pos(0, 1, 0), TORCH_GROUND_LIT);
    world.set_block(pos(0, 2, 0), REDSTONE_LAMP_LIT);

    // Unoptimized
    let expected = expect![[r#"
        digraph {
            0 [ label = "CompileNode { ty: Torch, block: Some((BlockPos { x: 0, y: 1, z: 0 }, 5738)), state: NodeState { powered: true, repeater_locked: false, output_strength: 15 }, is_input: false, is_output: false, annotations: Annotations }" ]
            1 [ label = "CompileNode { ty: Lamp, block: Some((BlockPos { x: 0, y: 2, z: 0 }, 7417)), state: NodeState { powered: true, repeater_locked: false, output_strength: 15 }, is_input: false, is_output: true, annotations: Annotations }" ]
            0 -> 1 [ label = "CompileLink { ty: Default, ss: 0 }" ]
        }
    "#]];
    test_frontend(&world, expected, OptLevel::Unoptimized);

    // Optimized
    let expected = expect![[r#"
        digraph {
            1 [ label = "CompileNode { ty: Lamp, block: Some((BlockPos { x: 0, y: 2, z: 0 }, 7417)), state: NodeState { powered: true, repeater_locked: false, output_strength: 15 }, is_input: false, is_output: true, annotations: Annotations }" ]
            2 [ label = "CompileNode { ty: Constant, block: None, state: NodeState { powered: false, repeater_locked: false, output_strength: 15 }, is_input: false, is_output: false, annotations: Annotations }" ]
            2 -> 1 [ label = "CompileLink { ty: Default, ss: 0 }" ]
        }
    "#]];
    test_frontend(&world, expected, OptLevel::Optimized);
}

#[test]
fn unlit_torch_on_redstone_block() {
    let mut world = TestWorld::new(1);
    world.set_block(pos(0, 0, 0), REDSTONE_BLOCK);
    world.set_block(pos(0, 1, 0), TORCH_GROUND_UNLIT);
    world.set_block(pos(0, 2, 0), REDSTONE_LAMP_UNLIT);

    // Unoptimized
    let expected = expect![[r#"
        digraph {
            0 [ label = "CompileNode { ty: Constant, block: Some((BlockPos { x: 0, y: 0, z: 0 }, 9223)), state: NodeState { powered: false, repeater_locked: false, output_strength: 15 }, is_input: false, is_output: false, annotations: Annotations }" ]
            1 [ label = "CompileNode { ty: Torch, block: Some((BlockPos { x: 0, y: 1, z: 0 }, 5739)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            2 [ label = "CompileNode { ty: Lamp, block: Some((BlockPos { x: 0, y: 2, z: 0 }, 7418)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: true, annotations: Annotations }" ]
            0 -> 1 [ label = "CompileLink { ty: Default, ss: 0 }" ]
            1 -> 2 [ label = "CompileLink { ty: Default, ss: 0 }" ]
        }
    "#]];
    test_frontend(&world, expected, OptLevel::Unoptimized);

    // Optimized
    let expected = expect![[r#"
        digraph {
            2 [ label = "CompileNode { ty: Lamp, block: Some((BlockPos { x: 0, y: 2, z: 0 }, 7418)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: true, annotations: Annotations }" ]
        }
    "#]];
    test_frontend(&world, expected, OptLevel::Optimized);
}

#[test]
fn redstone_lamp_with_multiple_connections_to_same_lever() {
    let mut world = TestWorld::new(1);
    world.set_block(pos(1, 1, 1), REDSTONE_LAMP_UNLIT);
    world.set_block(pos(1, 2, 1), LEVER_GROUND_DEACTIVATED);
    make_wire(&mut world, pos(0, 1, 1));
    make_wire(&mut world, pos(1, 1, 0));
    make_wire(&mut world, pos(2, 1, 1));
    make_wire(&mut world, pos(1, 1, 2));

    // Unoptimized
    let expected = expect![[r#"
        digraph {
            0 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 1 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            1 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 1, y: 1, z: 0 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            2 [ label = "CompileNode { ty: Lamp, block: Some((BlockPos { x: 1, y: 1, z: 1 }, 7418)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: true, annotations: Annotations }" ]
            3 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 1, y: 1, z: 2 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            4 [ label = "CompileNode { ty: Lever, block: Some((BlockPos { x: 1, y: 2, z: 1 }, 5627)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: true, is_output: false, annotations: Annotations }" ]
            5 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 2, y: 1, z: 1 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            4 -> 0 [ label = "CompileLink { ty: Default, ss: 0 }" ]
            4 -> 1 [ label = "CompileLink { ty: Default, ss: 0 }" ]
            4 -> 2 [ label = "CompileLink { ty: Default, ss: 0 }" ]
            4 -> 2 [ label = "CompileLink { ty: Default, ss: 0 }" ]
            4 -> 2 [ label = "CompileLink { ty: Default, ss: 0 }" ]
            4 -> 2 [ label = "CompileLink { ty: Default, ss: 0 }" ]
            4 -> 2 [ label = "CompileLink { ty: Default, ss: 0 }" ]
            4 -> 3 [ label = "CompileLink { ty: Default, ss: 0 }" ]
            4 -> 5 [ label = "CompileLink { ty: Default, ss: 0 }" ]
        }
    "#]];
    test_frontend(&world, expected, OptLevel::Unoptimized);

    // Optimized
    let expected = expect![[r#"
        digraph {
            0 [ label = "CompileNode { ty: Lamp, block: Some((BlockPos { x: 1, y: 1, z: 1 }, 7418)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: true, annotations: Annotations }" ]
            1 [ label = "CompileNode { ty: Lever, block: Some((BlockPos { x: 1, y: 2, z: 1 }, 5627)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: true, is_output: false, annotations: Annotations }" ]
            1 -> 0 [ label = "CompileLink { ty: Default, ss: 0 }" ]
        }
    "#]];
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
    let expected = expect![[r#"
        digraph {
            0 [ label = "CompileNode { ty: Lever, block: Some((BlockPos { x: 0, y: 1, z: 0 }, 5631)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: true, is_output: false, annotations: Annotations }" ]
            1 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 1 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            2 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 2 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            3 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 3 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            4 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 4 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            5 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 5 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            6 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 6 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            7 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 7 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            8 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 8 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            9 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 9 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            10 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 10 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            11 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 11 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            12 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 12 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            13 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 13 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            14 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 14 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            15 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 15 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            16 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 16 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            17 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 17 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            18 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 18 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            19 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 19 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            20 [ label = "CompileNode { ty: Lamp, block: Some((BlockPos { x: 0, y: 1, z: 20 }, 7418)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: true, annotations: Annotations }" ]
            0 -> 1 [ label = "CompileLink { ty: Default, ss: 0 }" ]
            0 -> 2 [ label = "CompileLink { ty: Default, ss: 1 }" ]
            0 -> 3 [ label = "CompileLink { ty: Default, ss: 2 }" ]
            0 -> 4 [ label = "CompileLink { ty: Default, ss: 3 }" ]
            0 -> 5 [ label = "CompileLink { ty: Default, ss: 4 }" ]
            0 -> 6 [ label = "CompileLink { ty: Default, ss: 5 }" ]
            0 -> 7 [ label = "CompileLink { ty: Default, ss: 6 }" ]
            0 -> 8 [ label = "CompileLink { ty: Default, ss: 7 }" ]
            0 -> 9 [ label = "CompileLink { ty: Default, ss: 8 }" ]
            0 -> 10 [ label = "CompileLink { ty: Default, ss: 9 }" ]
            0 -> 11 [ label = "CompileLink { ty: Default, ss: 10 }" ]
            0 -> 12 [ label = "CompileLink { ty: Default, ss: 11 }" ]
            0 -> 13 [ label = "CompileLink { ty: Default, ss: 12 }" ]
            0 -> 14 [ label = "CompileLink { ty: Default, ss: 13 }" ]
            0 -> 15 [ label = "CompileLink { ty: Default, ss: 14 }" ]
        }
    "#]];
    test_frontend(&world, expected, OptLevel::Unoptimized);

    // Optimized
    let expected = expect![[r#"
        digraph {
            0 [ label = "CompileNode { ty: Lever, block: Some((BlockPos { x: 0, y: 1, z: 0 }, 5631)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: true, is_output: false, annotations: Annotations }" ]
            1 [ label = "CompileNode { ty: Lamp, block: Some((BlockPos { x: 0, y: 1, z: 20 }, 7418)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: true, annotations: Annotations }" ]
        }
    "#]];
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
    let expected = expect![[r#"
        digraph {
            0 [ label = "CompileNode { ty: Lever, block: Some((BlockPos { x: 0, y: 1, z: 0 }, 5631)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: true, is_output: false, annotations: Annotations }" ]
            1 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 1 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            2 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 2 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            3 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 3 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            4 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 4 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            5 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 5 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            6 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 6 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            7 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 7 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            8 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 8 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            9 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 9 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            10 [ label = "CompileNode { ty: Comparator { mode: Compare, far_input: None, facing_diode: false }, block: Some((BlockPos { x: 0, y: 1, z: 10 }, 9176)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            11 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 11 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            12 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 12 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            13 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 13 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            14 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 14 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            15 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 15 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            0 -> 1 [ label = "CompileLink { ty: Default, ss: 0 }" ]
            0 -> 2 [ label = "CompileLink { ty: Default, ss: 1 }" ]
            0 -> 3 [ label = "CompileLink { ty: Default, ss: 2 }" ]
            0 -> 4 [ label = "CompileLink { ty: Default, ss: 3 }" ]
            0 -> 5 [ label = "CompileLink { ty: Default, ss: 4 }" ]
            0 -> 6 [ label = "CompileLink { ty: Default, ss: 5 }" ]
            0 -> 7 [ label = "CompileLink { ty: Default, ss: 6 }" ]
            0 -> 8 [ label = "CompileLink { ty: Default, ss: 7 }" ]
            0 -> 9 [ label = "CompileLink { ty: Default, ss: 8 }" ]
            0 -> 10 [ label = "CompileLink { ty: Default, ss: 8 }" ]
            10 -> 11 [ label = "CompileLink { ty: Default, ss: 0 }" ]
            10 -> 12 [ label = "CompileLink { ty: Default, ss: 1 }" ]
            10 -> 13 [ label = "CompileLink { ty: Default, ss: 2 }" ]
            10 -> 14 [ label = "CompileLink { ty: Default, ss: 3 }" ]
            10 -> 15 [ label = "CompileLink { ty: Default, ss: 4 }" ]
        }
    "#]];
    test_frontend(&world, expected, OptLevel::Unoptimized);

    // Optimized
    let expected = expect![[r#"
        digraph {
            0 [ label = "CompileNode { ty: Lever, block: Some((BlockPos { x: 0, y: 1, z: 0 }, 5631)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: true, is_output: false, annotations: Annotations }" ]
            1 [ label = "CompileNode { ty: Comparator { mode: Compare, far_input: None, facing_diode: false }, block: Some((BlockPos { x: 0, y: 1, z: 10 }, 9176)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            0 -> 1 [ label = "CompileLink { ty: Default, ss: 8 }" ]
        }
    "#]];
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
    let expected = expect![[r#"
        digraph {
            0 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 0 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            1 [ label = "CompileNode { ty: Repeater { delay: 1, facing_diode: false }, block: Some((BlockPos { x: 0, y: 1, z: 1 }, 5896)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            2 [ label = "CompileNode { ty: Lamp, block: Some((BlockPos { x: 0, y: 1, z: 2 }, 7418)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: true, annotations: Annotations }" ]
            3 [ label = "CompileNode { ty: Lamp, block: Some((BlockPos { x: 1, y: 1, z: 0 }, 7418)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: true, annotations: Annotations }" ]
            4 [ label = "CompileNode { ty: Repeater { delay: 1, facing_diode: false }, block: Some((BlockPos { x: 1, y: 1, z: 1 }, 5892)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            5 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 1, y: 1, z: 2 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
        }
    "#]];
    test_frontend(&world, expected, OptLevel::Unoptimized);

    // Optimized
    let expected = expect![[r#"
        digraph {
            1 [ label = "CompileNode { ty: Lamp, block: Some((BlockPos { x: 0, y: 1, z: 2 }, 7418)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: true, annotations: Annotations }" ]
            2 [ label = "CompileNode { ty: Lamp, block: Some((BlockPos { x: 1, y: 1, z: 0 }, 7418)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: true, annotations: Annotations }" ]
        }
    "#]];
    test_frontend(&world, expected, OptLevel::Optimized);
}

#[test]
fn comparator_compare_mode_with_full_ss_rear_input() {
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
    let expected = expect![[r#"
        digraph {
            0 [ label = "CompileNode { ty: Lamp, block: Some((BlockPos { x: 0, y: 1, z: 0 }, 7418)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: true, annotations: Annotations }" ]
            1 [ label = "CompileNode { ty: Comparator { mode: Compare, far_input: None, facing_diode: false }, block: Some((BlockPos { x: 0, y: 1, z: 1 }, 9180)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            2 [ label = "CompileNode { ty: Wire, block: Some((BlockPos { x: 0, y: 1, z: 2 }, 3558)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            3 [ label = "CompileNode { ty: Lever, block: Some((BlockPos { x: 0, y: 1, z: 3 }, 5631)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: true, is_output: false, annotations: Annotations }" ]
            4 [ label = "CompileNode { ty: Repeater { delay: 1, facing_diode: true }, block: Some((BlockPos { x: 1, y: 1, z: 1 }, 5896)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            5 [ label = "CompileNode { ty: Lever, block: Some((BlockPos { x: 2, y: 1, z: 1 }, 5631)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: true, is_output: false, annotations: Annotations }" ]
            1 -> 0 [ label = "CompileLink { ty: Default, ss: 0 }" ]
            4 -> 1 [ label = "CompileLink { ty: Side, ss: 0 }" ]
            3 -> 1 [ label = "CompileLink { ty: Default, ss: 0 }" ]
            3 -> 2 [ label = "CompileLink { ty: Default, ss: 0 }" ]
            5 -> 4 [ label = "CompileLink { ty: Default, ss: 0 }" ]
        }
    "#]];
    test_frontend(&world, expected, OptLevel::Unoptimized);

    // Optimized
    // TODO: Missed optimization:
    // The side input of the comparator has no effect on the comparator.
    let expected = expect![[r#"
        digraph {
            0 [ label = "CompileNode { ty: Lamp, block: Some((BlockPos { x: 0, y: 1, z: 0 }, 7418)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: true, annotations: Annotations }" ]
            1 [ label = "CompileNode { ty: Comparator { mode: Compare, far_input: None, facing_diode: false }, block: Some((BlockPos { x: 0, y: 1, z: 1 }, 9180)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            2 [ label = "CompileNode { ty: Lever, block: Some((BlockPos { x: 0, y: 1, z: 3 }, 5631)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: true, is_output: false, annotations: Annotations }" ]
            3 [ label = "CompileNode { ty: Repeater { delay: 1, facing_diode: true }, block: Some((BlockPos { x: 1, y: 1, z: 1 }, 5896)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: false, is_output: false, annotations: Annotations }" ]
            4 [ label = "CompileNode { ty: Lever, block: Some((BlockPos { x: 2, y: 1, z: 1 }, 5631)), state: NodeState { powered: false, repeater_locked: false, output_strength: 0 }, is_input: true, is_output: false, annotations: Annotations }" ]
            1 -> 0 [ label = "CompileLink { ty: Default, ss: 0 }" ]
            3 -> 1 [ label = "CompileLink { ty: Side, ss: 0 }" ]
            2 -> 1 [ label = "CompileLink { ty: Default, ss: 0 }" ]
            4 -> 3 [ label = "CompileLink { ty: Default, ss: 0 }" ]
        }
    "#]];
    test_frontend(&world, expected, OptLevel::Optimized);
}
