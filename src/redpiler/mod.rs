mod backend;
mod debug_graph;

use crate::blocks::{
    Block, BlockDirection, BlockEntity, BlockFace, BlockPos, ButtonFace, LeverFace,
};
use crate::plot::PlotWorld;
use crate::world::{TickEntry, World};
use backend::JITBackend;
use log::{error, warn};
use std::collections::{HashMap, VecDeque};

fn is_wire(world: &dyn World, pos: BlockPos) -> bool {
    matches!(world.get_block(pos), Block::RedstoneWire { .. })
}

type NodeId = usize;

#[derive(Debug, Clone)]
struct Link {
    start: NodeId,
    weight: u8,
    end: NodeId,
}

impl Link {
    fn new(start: NodeId, weight: u8, end: NodeId) -> Link {
        Link {
            start,
            weight,
            end,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompileNode {
    pos: BlockPos,
    state: Block,
    inputs: Vec<Link>,
    side_inputs: Vec<Link>,
    updates: Vec<NodeId>,
    comparator_output: u8,
    container_overriding: bool,
    facing_diode: bool,
    comparator_far_input: Option<u8>,
}

impl CompileNode {
    fn new(pos: BlockPos, state: Block, facing_diode: bool) -> CompileNode {
        CompileNode {
            pos,
            state,
            inputs: vec![],
            side_inputs: vec![],
            updates: vec![],
            comparator_output: 0,
            container_overriding: false,
            facing_diode,
            comparator_far_input: None,
        }
    }

    fn from_block(pos: BlockPos, block: Block, facing_diode: bool) -> Option<CompileNode> {
        let is_node = matches!(
            block,
            Block::RedstoneComparator { .. }
                | Block::RedstoneTorch { .. }
                | Block::RedstoneWallTorch { .. }
                | Block::RedstoneRepeater { .. }
                | Block::RedstoneWire { .. }
                | Block::Lever { .. }
                | Block::StoneButton { .. }
                | Block::RedstoneBlock { .. }
                | Block::RedstoneLamp { .. }
                | Block::StonePressurePlate { .. }
                | Block::IronTrapdoor { .. }
        );

        if is_node || block.has_comparator_override() {
            Some(CompileNode::new(pos, block, facing_diode))
        } else {
            None
        }
    }
}

struct InputSearch<'a> {
    plot: &'a mut PlotWorld,
    nodes: &'a mut Vec<CompileNode>,
    pos_map: HashMap<BlockPos, NodeId>,
}

impl<'a> InputSearch<'a> {
    fn new(plot: &'a mut PlotWorld, nodes: &'a mut Vec<CompileNode>) -> InputSearch<'a> {
        let mut pos_map = HashMap::new();
        for (i, node) in nodes.iter().enumerate() {
            pos_map.insert(node.pos, i);
        }

        InputSearch {
            plot,
            nodes,
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
                BlockFace::Top if lever.face == LeverFace::Floor => true,
                BlockFace::Bottom if lever.face == LeverFace::Ceiling => true,
                _ if lever.facing == side.to_direction() => true,
                _ => false,
            },
            Block::StoneButton { button } => match side {
                BlockFace::Top if button.face == ButtonFace::Floor => true,
                BlockFace::Bottom if button.face == ButtonFace::Ceiling => true,
                _ if button.facing == side.to_direction() => true,
                _ => false,
            },
            Block::RedstoneRepeater { .. } => self.provides_weak_power(block, side),
            Block::RedstoneComparator { .. } => self.provides_weak_power(block, side),
            _ => false,
        }
    }

    fn get_redstone_links(
        &self,
        block: Block,
        side: BlockFace,
        pos: BlockPos,
        distance: u8,
        start_node: NodeId,
        search_wire: bool,
    ) -> Vec<Link> {
        let mut res = Vec::new();
        if block.is_solid() {
            for side in &BlockFace::values() {
                let pos = pos.offset(*side);
                let block = self.plot.get_block(pos);
                if self.provides_strong_power(block, *side) {
                    res.push(Link::new(start_node, distance, self.pos_map[&pos]));
                }

                if let Block::RedstoneWire { wire } = block {
                    if !search_wire {
                        continue;
                    }
                    match side {
                        BlockFace::Top => {
                            res.append(&mut self.search_wire(start_node, pos, distance));
                        }
                        BlockFace::Bottom => {}
                        _ => {
                            let direction = side.to_direction();
                            if search_wire
                                && !wire
                                    .get_regulated_sides(self.plot, pos)
                                    .get_current_side(direction.opposite())
                                    .is_none()
                            {
                                res.append(
                                    &mut self.search_wire(start_node, pos, distance),
                                );
                            }
                        }
                    }
                }
            }
        } else if self.provides_weak_power(block, side) {
            res.push(Link::new(start_node, distance, self.pos_map[&pos]));
        } else if let Block::RedstoneWire { wire } = block {
            match side {
                BlockFace::Top => {
                    res.append(&mut self.search_wire(start_node, pos, distance))
                }
                BlockFace::Bottom => {}
                _ => {
                    let direction = side.to_direction();
                    if search_wire
                        && !wire
                            .get_regulated_sides(self.plot, pos)
                            .get_current_side(direction.opposite())
                            .is_none()
                    {
                        res.append(&mut self.search_wire(start_node, pos, distance));
                    }
                }
            }
        }
        res
    }

    fn search_wire(
        &self,
        start_node: NodeId,
        root_pos: BlockPos,
        mut distance: u8,
    ) -> Vec<Link> {
        let mut res = Vec::new();

        let mut queue: VecDeque<BlockPos> = VecDeque::new();
        let mut discovered = HashMap::new();

        discovered.insert(root_pos, distance);
        queue.push_back(root_pos);

        while !queue.is_empty() {
            let pos = queue.pop_front().unwrap();
            distance = discovered[&pos];

            let up_pos = pos.offset(BlockFace::Top);
            let up_block = self.plot.get_block(up_pos);

            for side in &BlockFace::values() {
                let neighbor_pos = pos.offset(*side);
                let neighbor = self.plot.get_block(neighbor_pos);

                res.append(&mut self.get_redstone_links(
                    neighbor,
                    *side,
                    neighbor_pos,
                    distance,
                    start_node,
                    false,
                ));

                if is_wire(self.plot, neighbor_pos) && !discovered.contains_key(&neighbor_pos) {
                    queue.push_back(neighbor_pos);
                    discovered.insert(neighbor_pos, discovered[&pos] + 1);
                }

                if side.is_horizontal() {
                    if !up_block.is_solid() && !neighbor.is_transparent() {
                        let neighbor_up_pos = neighbor_pos.offset(BlockFace::Top);
                        if is_wire(self.plot, neighbor_up_pos)
                            && !discovered.contains_key(&neighbor_up_pos)
                        {
                            queue.push_back(neighbor_up_pos);
                            discovered.insert(neighbor_up_pos, discovered[&pos] + 1);
                        }
                    }

                    if !neighbor.is_solid() {
                        let neighbor_down_pos = neighbor_pos.offset(BlockFace::Bottom);
                        if is_wire(self.plot, neighbor_down_pos)
                            && !discovered.contains_key(&neighbor_down_pos)
                        {
                            queue.push_back(neighbor_down_pos);
                            discovered.insert(neighbor_down_pos, discovered[&pos] + 1);
                        }
                    }
                }
            }
        }

        res
    }

    fn search_diode_inputs(
        &mut self,
        id: NodeId,
        pos: BlockPos,
        facing: BlockDirection,
    ) -> Vec<Link> {
        let input_pos = pos.offset(facing.block_face());
        let input_block = self.plot.get_block(input_pos);
        self.get_redstone_links(
            input_block,
            facing.block_face(),
            input_pos,
            0,
            id,
            true,
        )
    }

    fn search_repeater_side(
        &mut self,
        id: NodeId,
        pos: BlockPos,
        side: BlockDirection,
    ) -> Option<Link> {
        let side_pos = pos.offset(side.block_face());
        let side_block = self.plot.get_block(side_pos);
        if side_block.is_diode() && self.provides_weak_power(side_block, side.block_face()) {
            Some(Link::new(id, 0, self.pos_map[&side_pos]))
        } else {
            None
        }
    }

    fn search_comparator_side(
        &mut self,
        id: NodeId,
        pos: BlockPos,
        side: BlockDirection,
    ) -> Vec<Link> {
        let side_pos = pos.offset(side.block_face());
        let side_block = self.plot.get_block(side_pos);
        if side_block.is_diode() && self.provides_weak_power(side_block, side.block_face()) {
            vec![Link::new(id, 0, self.pos_map[&side_pos])]
        } else if matches!(side_block, Block::RedstoneWire { .. }) {
            self.search_wire(id, side_pos, 0)
        } else {
            vec![]
        }
    }

    fn search_node(&mut self, id: NodeId, node: CompileNode) {
        match node.state {
            Block::RedstoneTorch { .. } => {
                let bottom_pos = node.pos.offset(BlockFace::Bottom);
                let bottom_block = self.plot.get_block(bottom_pos);
                let inputs = self.get_redstone_links(
                    bottom_block,
                    BlockFace::Top,
                    bottom_pos,
                    0,
                    id,
                    true,
                );
                self.nodes[id].inputs = inputs;
            }
            Block::RedstoneWallTorch { facing, .. } => {
                let wall_pos = node.pos.offset(facing.opposite().block_face());
                let wall_block = self.plot.get_block(wall_pos);
                let inputs = self.get_redstone_links(
                    wall_block,
                    facing.opposite().block_face(),
                    wall_pos,
                    0,
                    id,
                    true,
                );
                self.nodes[id].inputs = inputs;
            }
            Block::RedstoneComparator { comparator } => {
                let facing = comparator.facing;

                let mut inputs = self.search_diode_inputs(id, node.pos, facing);
                let mut side_inputs = Vec::new();
                side_inputs.append(&mut self.search_comparator_side(id, node.pos, facing.rotate()));
                side_inputs.append(&mut self.search_comparator_side(id, node.pos, facing.rotate_ccw()));

                let input_pos = node.pos.offset(facing.block_face());
                let input_block = self.plot.get_block(input_pos);
                if input_block.has_comparator_override() {
                    self.nodes[id].container_overriding = true;
                    inputs.push(Link::new(
                        id,
                        0,
                        self.pos_map[&input_pos],
                    ));
                } else {
                    let far_input_pos = input_pos.offset(facing.block_face());
                    let far_input_block = self.plot.get_block(far_input_pos);
                    if input_block.is_solid() && far_input_block.has_comparator_override() {
                        let far_override =
                            far_input_block.get_comparator_override(self.plot, far_input_pos);
                        self.nodes[id].comparator_far_input = Some(far_override);
                    }
                }

                let output_strength = if let Some(BlockEntity::Comparator { output_strength }) =
                    self.plot.get_block_entity(node.pos)
                {
                    *output_strength
                } else {
                    0
                };

                self.nodes[id].comparator_output = output_strength;
                self.nodes[id].inputs = inputs;
                self.nodes[id].side_inputs = side_inputs;
            }
            Block::RedstoneRepeater { repeater } => {
                let facing = repeater.facing;

                let inputs = self.search_diode_inputs(id, node.pos, facing);
                let mut side_inputs = Vec::new();
                if let Some(l) = self.search_repeater_side(id, node.pos, facing.rotate()) {
                    side_inputs.push(l);
                }
                if let Some(l) = self.search_repeater_side(id, node.pos, facing.rotate_ccw()) {
                    side_inputs.push(l);
                }
                self.nodes[id].inputs = inputs;
                self.nodes[id].side_inputs = side_inputs;
            }
            Block::RedstoneWire { .. } => {
                let inputs = self.search_wire(id, node.pos, 0);
                self.nodes[id].inputs = inputs;
            }
            Block::RedstoneLamp { .. } | Block::IronTrapdoor { .. } => {
                let mut inputs = Vec::new();
                for face in &BlockFace::values() {
                    let neighbor_pos = node.pos.offset(*face);
                    let neighbor_block = self.plot.get_block(neighbor_pos);
                    let mut links = self.get_redstone_links(
                        neighbor_block,
                        *face,
                        neighbor_pos,
                        0,
                        id,
                        true,
                    );
                    inputs.append(&mut links);
                }
                self.nodes[id].inputs = inputs;
            }
            block if block.has_comparator_override() => {
                self.nodes[id].comparator_output =
                    block.get_comparator_override(self.plot, node.pos);
            }
            _ => {}
        }
    }

    fn search(&mut self) {
        let nodes = self.nodes.clone();
        for (i, node) in nodes.into_iter().enumerate() {
            self.search_node(i, node);
        }

        // Optimizations against the search graph like wire stripping and dedup go here

        // Dedup links
        let nodes = self.nodes.clone();
        for (i, node) in nodes.iter().enumerate() {
            let mut links: Vec<Link> = Vec::new();
            for link in node.inputs.clone() {
                let mut exists = false;
                for l in &mut links {
                    if l.end == link.end {
                        exists = true;
                        if link.weight < l.weight {
                            l.weight = link.weight;
                        }
                    }
                }

                if !exists && link.weight < 15 {
                    links.push(link);
                }
            }
            self.nodes[i].inputs = links;
        }
        for (i, node) in nodes.iter().enumerate() {
            let mut links: Vec<Link> = Vec::new();
            for link in node.side_inputs.clone() {
                let mut exists = false;
                for l in &mut links {
                    if l.end == link.end {
                        exists = true;
                        if link.weight < l.weight {
                            l.weight = link.weight;
                        }
                    }
                }

                if !exists && link.weight < 15 {
                    links.push(link);
                }
            }
            self.nodes[i].side_inputs = links;
        }

        // Remove other inputs to comparators with a comparator overriding container input.
        for (i, mut node) in self.nodes.clone().into_iter().enumerate() {
            if node.container_overriding {
                node.inputs.retain(|link| {
                    self.nodes[link.end].state.has_comparator_override()
                });
                self.nodes[i] = node;
            }
        }

        // Create update links
        for (id, node) in self.nodes.clone().into_iter().enumerate() {
            for input_node in node.inputs.into_iter().chain(node.side_inputs.into_iter()) {
                self.nodes[input_node.end].updates.push(id);
            }
        }
    }
}

#[derive(Default)]
pub struct CompilerOptions {
    pub optimize: bool,
    pub export: bool,
    pub io_only: bool,
}

impl CompilerOptions {
    pub fn parse(str: &str) -> CompilerOptions {
        let mut co: CompilerOptions = Default::default();
        let options = str.split_whitespace();
        for option in options {
            match option {
                "--no-wires" | "-O" => co.optimize = true,
                "--export" | "-E" => co.export = true,
                "--io-only" | "-I" => co.io_only = true,
                // FIXME: use actual error handling
                _ => warn!("Unrecognized option: {}", option),
            }
        }
        co
    }
}

#[derive(Default)]
pub struct Compiler {
    is_active: bool,
    jit: Option<Box<dyn JITBackend>>,
    options: CompilerOptions,
}

impl Compiler {
    pub fn is_active(&self) -> bool {
        self.is_active
    }

    /// Use just-in-time compilation with a `JITBackend` such as `CraneliftBackend` or `LLVMBackend`.
    /// Requires recompilation to take effect.
    pub fn use_jit(&mut self, jit: Box<dyn JITBackend>) {
        self.jit = Some(jit);
    }

    pub fn compile(
        &mut self,
        plot: &mut PlotWorld,
        options: CompilerOptions,
        ticks: Vec<TickEntry>,
    ) {
        let (first_pos, second_pos) = plot.get_corners();

        let mut nodes = Compiler::identify_nodes(plot, first_pos, second_pos, options.optimize);
        InputSearch::new(plot, &mut nodes).search();
        if options.export {
            debug_graph::debug(&nodes);
        }
        self.is_active = true;

        // TODO: Remove this once there is proper backend switching
        if self.jit.is_none() {
            let jit: Box<backend::direct::DirectBackend> = Default::default();
            // let jit: Box<codegen::cranelift::CraneliftBackend> = Default::default();
            self.use_jit(jit);
        }

        if let Some(jit) = &mut self.jit {
            jit.compile(nodes, ticks);
        } else {
            error!("Cannot compile without JIT variant selected");
        }

        self.options = options;
    }

    pub fn reset(&mut self, plot: &mut PlotWorld) {
        if self.is_active {
            self.is_active = false;
            if let Some(jit) = &mut self.jit {
                jit.reset(plot, self.options.io_only)
            }
        }

        if self.options.optimize {
            let (first_pos, second_pos) = plot.get_corners();
            let start_pos = first_pos.min(second_pos);
            let end_pos = first_pos.max(second_pos);
            for y in start_pos.y..=end_pos.y {
                for z in start_pos.z..=end_pos.z {
                    for x in start_pos.x..=end_pos.x {
                        let pos = BlockPos::new(x, y, z);
                        let block = plot.get_block(pos);
                        if matches!(block, Block::RedstoneWire { .. }) {
                            block.update(plot, pos);
                        }
                    }
                }
            }
        }
        self.options = Default::default();
    }

    fn backend(&mut self) -> &mut Box<dyn JITBackend> {
        assert!(
            self.is_active,
            "tried to get redpiler backend when inactive"
        );
        if let Some(jit) = &mut self.jit {
            jit
        } else {
            panic!("redpiler is active but is missing jit backend");
        }
    }

    pub fn tick(&mut self, plot: &mut PlotWorld) {
        self.backend().tick(plot);
    }

    pub fn on_use_block(&mut self, plot: &mut PlotWorld, pos: BlockPos) {
        self.backend().on_use_block(plot, pos);
    }

    pub fn set_pressure_plate(&mut self, plot: &mut PlotWorld, pos: BlockPos, powered: bool) {
        self.backend().set_pressure_plate(plot, pos, powered);
    }

    pub fn flush(&mut self, plot: &mut PlotWorld) {
        let io_only = self.options.io_only;
        self.backend().flush(plot, io_only);
    }

    fn identify_nodes(
        plot: &mut PlotWorld,
        first_pos: BlockPos,
        second_pos: BlockPos,
        no_wires: bool,
    ) -> Vec<CompileNode> {
        let mut nodes = Vec::new();
        let start_pos = first_pos.min(second_pos);
        let end_pos = first_pos.max(second_pos);
        for y in start_pos.y..=end_pos.y {
            for z in start_pos.z..=end_pos.z {
                for x in start_pos.x..=end_pos.x {
                    let pos = BlockPos::new(x, y, z);
                    let block = plot.get_block(pos);
                    let facing_diode = if let Block::RedstoneRepeater { repeater } = block {
                        plot.get_block(pos.offset(repeater.facing.opposite().block_face()))
                            .is_diode()
                    } else if let Block::RedstoneComparator { comparator } = block {
                        plot.get_block(pos.offset(comparator.facing.opposite().block_face()))
                            .is_diode()
                    } else {
                        false
                    };

                    if no_wires && matches!(block, Block::RedstoneWire { .. }) {
                        continue;
                    }

                    if let Some(node) = CompileNode::from_block(pos, block, facing_diode) {
                        nodes.push(node);
                    }
                }
            }
        }
        nodes
    }
}
