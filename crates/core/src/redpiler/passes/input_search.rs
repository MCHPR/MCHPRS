//! # [`InputSearch`]
//!
//! This pass populates the graph with edges.
//! This pass is *mandatory*. Without it, there would be no links between nodes.

use super::Pass;
use crate::redpiler::compile_graph::{CompileGraph, CompileLink, LinkType, NodeIdx};
use crate::redpiler::{CompilerInput, CompilerOptions};
use crate::redstone::{self, comparator, wire};
use crate::world::World;
use mchprs_blocks::blocks::{Block, ButtonFace, LeverFace};
use mchprs_blocks::{BlockDirection, BlockFace, BlockPos};
use petgraph::visit::NodeIndexable;
use rustc_hash::FxHashMap;
use std::collections::VecDeque;

pub struct InputSearch;

impl<W: World> Pass<W> for InputSearch {
    fn run_pass(
        &self,
        graph: &mut CompileGraph,
        _: &CompilerOptions,
        input: &CompilerInput<'_, W>,
    ) {
        let mut state = InputSearchState::new(input.world, graph);
        state.search();
    }

    fn should_run(&self, _: &CompilerOptions) -> bool {
        // Mandatory
        true
    }

    fn status_message(&self) -> &'static str {
        "Searching for links"
    }
}

struct InputSearchState<'a, W: World> {
    world: &'a W,
    graph: &'a mut CompileGraph,
    pos_map: FxHashMap<BlockPos, NodeIdx>,
}

impl<'a, W: World> InputSearchState<'a, W> {
    fn new(world: &'a W, graph: &'a mut CompileGraph) -> InputSearchState<'a, W> {
        let mut pos_map = FxHashMap::default();
        for id in graph.node_indices() {
            let (pos, _) = graph[id].block.unwrap();
            pos_map.insert(pos, id);
        }

        InputSearchState {
            world,
            graph,
            pos_map,
        }
    }

    fn provides_weak_power(&self, block: Block, side: BlockFace) -> bool {
        match block {
            Block::RedstoneTorch { .. } => true,
            Block::RedstoneWallTorch { facing, .. } if facing.block_face() != side => true,
            Block::RedstoneBlock {} => true,
            Block::Lever { .. } => true,
            Block::StoneButton { .. } => true,
            Block::StonePressurePlate { .. } => true,
            Block::RedstoneRepeater { repeater } if repeater.facing.block_face() == side => true,
            Block::RedstoneComparator { comparator } if comparator.facing.block_face() == side => {
                true
            }
            _ => false,
        }
    }

    fn provides_strong_power(&self, block: Block, side: BlockFace) -> bool {
        match block {
            Block::RedstoneTorch { .. } if side == BlockFace::Bottom => true,
            Block::RedstoneWallTorch { .. } if side == BlockFace::Bottom => true,
            Block::StonePressurePlate { .. } if side == BlockFace::Top => true,
            Block::Lever { lever } => match side {
                BlockFace::Top => lever.face == LeverFace::Floor,
                BlockFace::Bottom => lever.face == LeverFace::Ceiling,
                _ => lever.face == LeverFace::Wall && lever.facing == side.unwrap_direction(),
            },
            Block::StoneButton { button } => match side {
                BlockFace::Top => button.face == ButtonFace::Floor,
                BlockFace::Bottom => button.face == ButtonFace::Ceiling,
                _ => button.face == ButtonFace::Wall && button.facing == side.unwrap_direction(),
            },
            Block::RedstoneRepeater { .. } => self.provides_weak_power(block, side),
            Block::RedstoneComparator { .. } => self.provides_weak_power(block, side),
            _ => false,
        }
    }

    // unfortunate
    #[allow(clippy::too_many_arguments)]
    fn get_redstone_links(
        &mut self,
        block: Block,
        side: BlockFace,
        pos: BlockPos,
        link_ty: LinkType,
        distance: u8,
        start_node: NodeIdx,
        search_wire: bool,
    ) {
        if block.is_solid() {
            for side in &BlockFace::values() {
                let pos = pos.offset(*side);
                let block = self.world.get_block(pos);
                if self.provides_strong_power(block, *side) {
                    self.graph.add_edge(
                        self.pos_map[&pos],
                        start_node,
                        CompileLink::new(link_ty, distance),
                    );
                }

                if let Block::RedstoneWire { wire } = block {
                    if !search_wire {
                        continue;
                    }
                    match side {
                        BlockFace::Top => {
                            self.search_wire(start_node, pos, link_ty, distance);
                        }
                        BlockFace::Bottom => {}
                        _ => {
                            let direction = side.unwrap_direction();
                            if search_wire
                                && !wire::get_current_side(
                                    wire::get_regulated_sides(wire, self.world, pos),
                                    direction.opposite(),
                                )
                                .is_none()
                            {
                                self.search_wire(start_node, pos, link_ty, distance);
                            }
                        }
                    }
                }
            }
        } else if self.provides_weak_power(block, side) {
            self.graph.add_edge(
                self.pos_map[&pos],
                start_node,
                CompileLink::new(link_ty, distance),
            );
        } else if let Block::RedstoneWire { wire } = block {
            match side {
                BlockFace::Top => self.search_wire(start_node, pos, link_ty, distance),
                BlockFace::Bottom => {}
                _ => {
                    let direction = side.unwrap_direction();
                    if search_wire
                        && !wire::get_current_side(
                            wire::get_regulated_sides(wire, self.world, pos),
                            direction.opposite(),
                        )
                        .is_none()
                    {
                        self.search_wire(start_node, pos, link_ty, distance);
                    }
                }
            }
        }
    }

    fn search_wire(
        &mut self,
        start_node: NodeIdx,
        root_pos: BlockPos,
        link_ty: LinkType,
        mut distance: u8,
    ) {
        let mut queue: VecDeque<BlockPos> = VecDeque::new();
        let mut discovered = FxHashMap::default();

        discovered.insert(root_pos, distance);
        queue.push_back(root_pos);

        while !queue.is_empty() {
            let pos = queue.pop_front().unwrap();
            distance = discovered[&pos];

            let up_pos = pos.offset(BlockFace::Top);
            let up_block = self.world.get_block(up_pos);

            for side in &BlockFace::values() {
                let neighbor_pos = pos.offset(*side);
                let neighbor = self.world.get_block(neighbor_pos);

                self.get_redstone_links(
                    neighbor,
                    *side,
                    neighbor_pos,
                    link_ty,
                    distance,
                    start_node,
                    false,
                );

                if is_wire(self.world, neighbor_pos) && !discovered.contains_key(&neighbor_pos) {
                    queue.push_back(neighbor_pos);
                    discovered.insert(neighbor_pos, discovered[&pos] + 1);
                }

                if side.is_horizontal() {
                    if !up_block.is_solid() && !neighbor.is_transparent() {
                        let neighbor_up_pos = neighbor_pos.offset(BlockFace::Top);
                        if is_wire(self.world, neighbor_up_pos)
                            && !discovered.contains_key(&neighbor_up_pos)
                        {
                            queue.push_back(neighbor_up_pos);
                            discovered.insert(neighbor_up_pos, discovered[&pos] + 1);
                        }
                    }

                    if !neighbor.is_solid() {
                        let neighbor_down_pos = neighbor_pos.offset(BlockFace::Bottom);
                        if is_wire(self.world, neighbor_down_pos)
                            && !discovered.contains_key(&neighbor_down_pos)
                        {
                            queue.push_back(neighbor_down_pos);
                            discovered.insert(neighbor_down_pos, discovered[&pos] + 1);
                        }
                    }
                }
            }
        }
    }

    fn search_diode_inputs(&mut self, id: NodeIdx, pos: BlockPos, facing: BlockDirection) {
        let input_pos = pos.offset(facing.block_face());
        let input_block = self.world.get_block(input_pos);
        self.get_redstone_links(
            input_block,
            facing.block_face(),
            input_pos,
            LinkType::Default,
            0,
            id,
            true,
        )
    }

    fn search_repeater_side(&mut self, id: NodeIdx, pos: BlockPos, side: BlockDirection) {
        let side_pos = pos.offset(side.block_face());
        let side_block = self.world.get_block(side_pos);
        if redstone::is_diode(side_block) && self.provides_weak_power(side_block, side.block_face())
        {
            self.graph
                .add_edge(self.pos_map[&side_pos], id, CompileLink::side(0));
        }
    }

    fn search_comparator_side(&mut self, id: NodeIdx, pos: BlockPos, side: BlockDirection) {
        let side_pos = pos.offset(side.block_face());
        let side_block = self.world.get_block(side_pos);
        if (redstone::is_diode(side_block)
            && self.provides_weak_power(side_block, side.block_face()))
            || matches!(side_block, Block::RedstoneBlock { .. })
        {
            self.graph
                .add_edge(self.pos_map[&side_pos], id, CompileLink::side(0));
        } else if matches!(side_block, Block::RedstoneWire { .. }) {
            self.search_wire(id, side_pos, LinkType::Side, 0)
        }
    }

    fn search_node(&mut self, id: NodeIdx, (pos, block_id): (BlockPos, u32)) {
        match Block::from_id(block_id) {
            Block::RedstoneTorch { .. } => {
                let bottom_pos = pos.offset(BlockFace::Bottom);
                let bottom_block = self.world.get_block(bottom_pos);
                self.get_redstone_links(
                    bottom_block,
                    BlockFace::Top,
                    bottom_pos,
                    LinkType::Default,
                    0,
                    id,
                    true,
                );
            }
            Block::RedstoneWallTorch { facing, .. } => {
                let wall_pos = pos.offset(facing.opposite().block_face());
                let wall_block = self.world.get_block(wall_pos);
                self.get_redstone_links(
                    wall_block,
                    facing.opposite().block_face(),
                    wall_pos,
                    LinkType::Default,
                    0,
                    id,
                    true,
                );
            }
            Block::RedstoneComparator { comparator } => {
                let facing = comparator.facing;

                self.search_comparator_side(id, pos, facing.rotate());
                self.search_comparator_side(id, pos, facing.rotate_ccw());

                let input_pos = pos.offset(facing.block_face());
                let input_block = self.world.get_block(input_pos);
                if comparator::has_override(input_block) {
                    self.graph
                        .add_edge(self.pos_map[&input_pos], id, CompileLink::default(0));
                } else {
                    self.search_diode_inputs(id, pos, facing);
                }
            }
            Block::RedstoneRepeater { repeater } => {
                let facing = repeater.facing;

                self.search_diode_inputs(id, pos, facing);
                self.search_repeater_side(id, pos, facing.rotate());
                self.search_repeater_side(id, pos, facing.rotate_ccw());
            }
            Block::RedstoneWire { .. } => {
                self.search_wire(id, pos, LinkType::Default, 0);
            }
            Block::RedstoneLamp { .. } | Block::IronTrapdoor { .. } | Block::NoteBlock { .. } => {
                for face in &BlockFace::values() {
                    let neighbor_pos = pos.offset(*face);
                    let neighbor_block = self.world.get_block(neighbor_pos);
                    self.get_redstone_links(
                        neighbor_block,
                        *face,
                        neighbor_pos,
                        LinkType::Default,
                        0,
                        id,
                        true,
                    );
                }
            }
            _ => {}
        }
    }

    fn search(&mut self) {
        for i in 0..self.graph.node_bound() {
            let idx = NodeIdx::new(i);
            if !self.graph.contains_node(idx) {
                continue;
            }
            let node = &self.graph[idx];
            self.search_node(idx, node.block.unwrap());
        }
    }
}

fn is_wire(world: &impl World, pos: BlockPos) -> bool {
    matches!(world.get_block(pos), Block::RedstoneWire { .. })
}
