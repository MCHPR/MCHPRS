//! # [`IdentifyNodes`]
//!
//! This pass populates the graph with nodes using the input given in [`CompilerInput`].
//! This pass is *mandatory*. Without it, the graph will never be populated.
//!
//! If `optimize` is set in [`CompilerOptions`], redstone wires will not be added to the graph.
//!
//! There are no requirements for this pass.

use super::Pass;
use crate::redpiler::compile_graph::{CompileGraph, CompileNode, NodeState, NodeType};
use crate::redpiler::{CompilerInput, CompilerOptions};
use crate::redstone;
use crate::world::{for_each_block_optimized, World};
use mchprs_blocks::block_entities::BlockEntity;
use mchprs_blocks::blocks::{Block, RedstoneComparator, RedstoneRepeater};
use mchprs_blocks::BlockPos;

pub struct IdentifyNodes;

impl<W: World> Pass<W> for IdentifyNodes {
    fn run_pass(
        &self,
        graph: &mut CompileGraph,
        options: &CompilerOptions,
        input: &CompilerInput<'_, W>,
    ) {
        let ignore_wires = options.optimize;
        let plot = input.world;

        let (first_pos, second_pos) = input.bounds;

        let start_pos = first_pos.min(second_pos);
        let end_pos = first_pos.max(second_pos);

        for_each_block_optimized(plot, start_pos, end_pos, |pos| {
            for_pos(ignore_wires, plot, graph, pos)
        });
    }

    fn should_run(&self, _: &CompilerOptions) -> bool {
        // Mandatory
        true
    }
}

fn for_pos<W: World>(ignore_wires: bool, world: &W, graph: &mut CompileGraph, pos: BlockPos) {
    let id = world.get_block_raw(pos);
    let block = Block::from_id(id);

    let Some((ty, state)) = identify_block(block, pos, world) else {
        return;
    };

    let facing_diode = match block {
        Block::RedstoneRepeater {
            repeater: RedstoneRepeater { facing, .. },
            ..
        }
        | Block::RedstoneComparator {
            comparator: RedstoneComparator { facing, .. },
            ..
        } => {
            let facing_block = world.get_block(pos.offset(facing.opposite().block_face()));
            redstone::is_diode(facing_block)
        }
        _ => false,
    };

    if ignore_wires && ty == NodeType::Wire {
        return;
    }

    graph.add_node(CompileNode {
        ty,
        block: Some((pos, id)),
        state,

        facing_diode,
        comparator_far_input: None,
    });
}

fn identify_block<W: World>(
    block: Block,
    pos: BlockPos,
    world: &W,
) -> Option<(NodeType, NodeState)> {
    let (ty, state) = match block {
        Block::RedstoneRepeater { repeater } => (
            NodeType::Repeater(repeater.delay),
            NodeState::repeater(repeater.powered, repeater.locked),
        ),
        Block::RedstoneComparator { comparator } => (
            NodeType::Comparator(comparator.mode),
            NodeState::comparator(
                comparator.powered,
                if let Some(BlockEntity::Comparator { output_strength }) =
                    world.get_block_entity(pos)
                {
                    *output_strength
                } else {
                    0
                },
            ),
        ),
        Block::RedstoneTorch { lit, .. } | Block::RedstoneWallTorch { lit, .. } => {
            (NodeType::Torch, NodeState::simple(lit))
        }
        Block::RedstoneWire { wire } => (NodeType::Wire, NodeState::ss(wire.power)),
        Block::StoneButton { button } => (NodeType::Button, NodeState::simple(button.powered)),
        Block::RedstoneLamp { lit } => (NodeType::Lamp, NodeState::simple(lit)),
        Block::Lever { lever } => (NodeType::Lever, NodeState::simple(lever.powered)),
        Block::StonePressurePlate { powered } => {
            (NodeType::PressurePlate, NodeState::simple(powered))
        }
        Block::IronTrapdoor { powered, .. } => (NodeType::Trapdoor, NodeState::simple(powered)),
        Block::RedstoneBlock {} => (NodeType::Constant, NodeState::ss(15)),
        block if redstone::has_comparator_override(block) => (
            NodeType::Constant,
            NodeState::ss(redstone::get_comparator_override(block, world, pos)),
        ),
        _ => return None,
    };
    Some((ty, state))
}
