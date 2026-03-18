//! # [`IdentifyNodes`]
//!
//! This pass populates the graph with nodes using the input given in [`CompilerInput`].
//! This pass is *mandatory*. Without it, the graph will never be populated.
//!
//! If `optimize` is set in [`CompilerOptions`], redstone wires will not be added to the graph.
//!
//! There are no requirements for this pass.

use crate::compile_graph::{Annotations, CompileGraph, CompileNode, NodeIdx, NodeState, NodeType};
use crate::passes::{AnalysisInfos, Pass};
use crate::{CompilerInput, CompilerOptions};
use itertools::Itertools;
use mchprs_blocks::block_entities::BlockEntity;
use mchprs_blocks::blocks::Block;
use mchprs_blocks::{BlockDirection, BlockFace, BlockPos};
use mchprs_redstone::{self, comparator, noteblock, wire};
use mchprs_world::{for_each_block_optimized, World};
use rustc_hash::{FxHashMap, FxHashSet};
use serde_json::Value;
use tracing::warn;

pub struct IdentifyNodes;

impl<W: World> Pass<W> for IdentifyNodes {
    fn run_pass(
        &self,
        graph: &mut CompileGraph,
        options: &CompilerOptions,
        input: &CompilerInput<'_, W>,
        _: &mut AnalysisInfos,
    ) {
        let ignore_wires = options.optimize;
        let plot = input.world;

        let mut first_pass = FxHashMap::default();
        let mut second_pass = FxHashSet::default();

        let (first_pos, second_pos) = input.bounds;

        for_each_block_optimized(plot, first_pos, second_pos, |pos| {
            for_pos(
                graph,
                &mut first_pass,
                &mut second_pass,
                ignore_wires,
                options.wire_dot_out,
                plot,
                pos,
            );
        });

        for pos in second_pass {
            apply_annotations(graph, options, &first_pass, plot, pos);
        }
    }

    fn status_message(&self) -> &'static str {
        "Identifying nodes"
    }

    fn driver_key(&self) -> &'static str {
        "identify-nodes"
    }
}

fn for_pos<W: World>(
    graph: &mut CompileGraph,
    first_pass: &mut FxHashMap<BlockPos, NodeIdx>,
    second_pass: &mut FxHashSet<BlockPos>,
    ignore_wires: bool,
    wire_dot_out: bool,
    world: &W,
    pos: BlockPos,
) {
    let id = world.get_block_raw(pos);
    let block = Block::from_id(id);

    if block.is_sign() || block.is_wall_sign() {
        second_pass.insert(pos);
        return;
    }

    let Some((ty, state)) = identify_block(block, pos, world) else {
        return;
    };

    let is_input = ty.is_normally_input();
    let is_output = ty.is_normally_output()
        || matches!(block, Block::RedstoneWire(wire) if wire_dot_out && wire::is_dot(wire));

    if ignore_wires && ty == NodeType::Wire && !(is_input | is_output) {
        return;
    }

    let node_idx = graph.add_node(CompileNode {
        ty,
        block: Some((pos, id)),
        name: None,
        state,

        is_input,
        is_output,
        annotations: Annotations::default(),
    });
    first_pass.insert(pos, node_idx);
}

fn identify_block<W: World>(
    block: Block,
    pos: BlockPos,
    world: &W,
) -> Option<(NodeType, NodeState)> {
    if let Some(powered) = block.clone().get_pressure_plate_powered() {
        return Some((NodeType::PressurePlate, NodeState::simple(*powered)));
    }
    let (ty, state) = match block {
        Block::Repeater(repeater) => (
            NodeType::Repeater {
                delay: repeater.delay,
                facing_diode: mchprs_redstone::is_diode(
                    world.get_block(pos.offset(repeater.facing.opposite().block_face())),
                ),
            },
            NodeState::repeater(repeater.powered, repeater.locked),
        ),
        Block::Comparator(comparator) => (
            NodeType::Comparator {
                mode: comparator.mode,
                far_input: comparator::get_far_input(world, pos, comparator.facing),
                facing_diode: mchprs_redstone::is_diode(
                    world.get_block(pos.offset(comparator.facing.opposite().block_face())),
                ),
            },
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
        Block::RedstoneWire(wire) => (NodeType::Wire, NodeState::ss(wire.power)),
        Block::StoneButton { powered, .. } => (NodeType::Button, NodeState::simple(powered)),
        Block::RedstoneLamp { lit } => (NodeType::Lamp, NodeState::simple(lit)),
        Block::Lever { powered, .. } => (NodeType::Lever, NodeState::simple(powered)),
        Block::IronTrapdoor { powered, .. } => (NodeType::Trapdoor, NodeState::simple(powered)),
        Block::RedstoneBlock => (NodeType::Constant, NodeState::ss(15)),
        Block::NoteBlock {
            instrument: _,
            note,
            powered,
        } if noteblock::is_noteblock_unblocked(world, pos) => {
            let instrument = noteblock::get_noteblock_instrument(world, pos);
            (
                NodeType::NoteBlock { instrument, note },
                NodeState::simple(powered),
            )
        }
        block if comparator::has_override(block) => (
            NodeType::Constant,
            NodeState::ss(comparator::get_override(block, world, pos)),
        ),
        _ => return None,
    };
    Some((ty, state))
}

fn apply_annotations<W: World>(
    graph: &mut CompileGraph,
    options: &CompilerOptions,
    first_pass: &FxHashMap<BlockPos, NodeIdx>,
    world: &W,
    pos: BlockPos,
) {
    let block = world.get_block(pos);
    let annotations = parse_sign_annotations(world.get_block_entity(pos));
    if annotations.is_empty() {
        return;
    }

    let targets = match (block.get_sign_rotation(), block.get_wall_sign_facing()) {
        (Some(rotation), None) => {
            if let Some(facing) = BlockDirection::from_rotation(rotation) {
                let behind = pos.offset(facing.opposite().block_face());
                vec![behind]
            } else {
                warn!("Found sign with annotations, but bad rotation at {}", pos);
                return;
            }
        }
        (None, Some(facing)) => {
            let behind = pos.offset(facing.opposite().block_face());
            vec![
                behind,
                behind.offset(BlockFace::Top),
                behind.offset(BlockFace::Bottom),
            ]
        }
        _ => panic!("Block unimplemented for second pass"),
    };

    let target = targets.iter().flat_map(|pos| first_pass.get(pos)).next();
    if let Some(&node_idx) = target {
        for annotation in annotations {
            let result = annotation.apply(graph, node_idx, options);
            if let Err(msg) = result {
                warn!("{} at {}", msg, pos);
            }
        }
    } else {
        warn!("Could not find component for annotation at {}", pos);
    }
}

fn parse_sign_annotations(entity: Option<&BlockEntity>) -> Vec<NodeAnnotation> {
    if let Some(BlockEntity::Sign(sign)) = entity {
        sign.front_rows
            .iter()
            .flat_map(|row| serde_json::from_str(row))
            .flat_map(|json: Value| NodeAnnotation::parse(json.as_object()?.get("text")?.as_str()?))
            .collect_vec()
    } else {
        vec![]
    }
}

pub enum NodeAnnotation {}

impl NodeAnnotation {
    fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        if !(s.starts_with('[') && s.ends_with(']')) {
            return None;
        }
        let _parts = s[1..s.len() - 1].split(' ').collect_vec();
        None
    }

    fn apply(
        self,
        _graph: &mut CompileGraph,
        _node_idx: NodeIdx,
        _options: &CompilerOptions,
    ) -> Result<(), String> {
        match self {}
    }
}
