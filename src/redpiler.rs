use crate::blocks::{
    Block, BlockDirection, BlockFace, BlockPos, ButtonFace, ComparatorMode, LeverFace,
    RedstoneComparator,
};
use crate::plot::Plot;
use crate::world::{TickPriority, World};
use std::collections::{HashMap, VecDeque};
use std::fmt::Display;

fn is_wire(world: &dyn World, pos: BlockPos) -> bool {
    matches!(world.get_block(pos), Block::RedstoneWire { .. })
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct NodeId {
    index: usize,
}

#[derive(Debug, Clone, Copy)]
enum LinkType {
    Default,
    Side,
}

#[derive(Debug, Clone)]
struct Link {
    ty: LinkType,
    start: NodeId,
    weight: u8,
    end: NodeId,
}

impl Link {
    fn new(ty: LinkType, start: NodeId, weight: u8, end: NodeId) -> Link {
        Link {
            ty,
            start,
            weight,
            end,
        }
    }
}

#[derive(Debug, Clone)]
struct Node {
    pos: BlockPos,
    state: Block,
    inputs: Vec<Link>,
    updates: Vec<NodeId>,
    comparator_output: u8,
    facing_diode: bool,
}

impl Node {
    fn new(pos: BlockPos, state: Block, facing_diode: bool) -> Node {
        Node {
            pos,
            state,
            inputs: vec![],
            updates: vec![],
            comparator_output: 0,
            facing_diode,
        }
    }

    fn from_block(pos: BlockPos, block: Block, facing_diode: bool) -> Option<Node> {
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
        );

        if is_node || block.has_comparator_override() {
            Some(Node::new(pos, block, facing_diode))
        } else {
            None
        }
    }

    fn get_output_power(&self) -> u8 {
        match self.state {
            Block::RedstoneComparator { .. } => self.comparator_output,
            Block::RedstoneTorch { lit } => lit.then(|| 15).unwrap_or(0),
            Block::RedstoneWallTorch { lit, .. } => lit.then(|| 15).unwrap_or(0),
            Block::RedstoneRepeater { repeater } => repeater.powered.then(|| 15).unwrap_or(0),
            Block::Lever { lever } => lever.powered.then(|| 15).unwrap_or(0),
            Block::StoneButton { button } => button.powered.then(|| 15).unwrap_or(0),
            Block::RedstoneBlock {} => 15,
            s if s.has_comparator_override() => self.comparator_output,
            s => panic!("How did {:?} become an output node?", s),
        }
    }
}

struct InputSearch<'a> {
    plot: &'a mut Plot,
    pos_map: HashMap<BlockPos, NodeId>,
}

impl<'a> InputSearch<'a> {
    fn new(plot: &mut Plot) -> InputSearch<'_> {
        let compiler = &mut plot.redpiler;
        let nodes = &mut compiler.nodes;

        let mut pos_map = HashMap::new();
        for (i, node) in nodes.iter().enumerate() {
            pos_map.insert(node.pos, NodeId { index: i });
        }

        compiler.pos_map.clone_from(&pos_map);

        InputSearch { plot, pos_map }
    }

    fn provides_weak_power(&self, block: Block, side: BlockFace) -> bool {
        match block {
            Block::RedstoneTorch { .. } => true,
            Block::RedstoneWallTorch { facing, .. } if facing.block_face() != side => true,
            Block::RedstoneBlock {} => true,
            Block::Lever { .. } => true,
            Block::StoneButton { .. } => true,
            Block::RedstoneRepeater { repeater } if repeater.facing.block_face() == side => true,
            Block::RedstoneComparator { comparator } if comparator.facing.block_face() == side => {
                true
            }
            _ => false,
        }
    }

    fn provides_strong_power(&self, block: Block, side: BlockFace) -> bool {
        match block {
            Block::RedstoneTorch { lit: true } if side == BlockFace::Bottom => true,
            Block::RedstoneWallTorch { lit: true, .. } if side == BlockFace::Bottom => true,
            Block::Lever { lever } => match side {
                BlockFace::Top if lever.face == LeverFace::Floor => true,
                BlockFace::Bottom if lever.face == LeverFace::Ceiling => true,
                _ if lever.facing == side.to_direction() => true,
                _ => false,
            },
            Block::StoneButton { button } => match side {
                BlockFace::Top if button.face == ButtonFace::Floor && button.powered => true,
                BlockFace::Bottom if button.face == ButtonFace::Ceiling && button.powered => true,
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
        link_ty: LinkType,
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
                    res.push(Link::new(link_ty, start_node, distance, self.pos_map[&pos]));
                }

                if let Block::RedstoneWire { wire } = block {
                    if !search_wire {
                        continue;
                    }
                    match side {
                        BlockFace::Top => {
                            res.append(&mut self.search_wire(start_node, pos, link_ty, distance));
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
                                res.append(&mut self.search_wire(start_node, pos, link_ty, distance));
                            }
                        }
                    }
                }
            }
        } else if self.provides_weak_power(block, side) {
            res.push(Link::new(link_ty, start_node, distance, self.pos_map[&pos]));
        } else if let Block::RedstoneWire { wire } = block {
            let direction = side.to_direction();
            if search_wire
                && !wire
                    .get_regulated_sides(self.plot, pos)
                    .get_current_side(direction.opposite())
                    .is_none()
            {
                res.append(&mut self.search_wire(start_node, pos, link_ty, distance));
            }
        }
        res
    }

    fn search_wire(
        &self,
        start_node: NodeId,
        root_pos: BlockPos,
        link_ty: LinkType,
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
                    link_ty,
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
            LinkType::Default,
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
            Some(Link::new(LinkType::Side, id, 0, self.pos_map[&side_pos]))
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
            vec![Link::new(LinkType::Side, id, 0, self.pos_map[&side_pos])]
        } else if matches!(side_block, Block::RedstoneWire { .. }) {
            self.search_wire(id, side_pos, LinkType::Side, 0)
        } else {
            vec![]
        }
    }

    fn search_node(&mut self, id: NodeId, node: Node) {
        match node.state {
            Block::RedstoneTorch { .. } => {
                let bottom_pos = node.pos.offset(BlockFace::Bottom);
                let bottom_block = self.plot.get_block(bottom_pos);
                let inputs = self.get_redstone_links(
                    bottom_block,
                    BlockFace::Top,
                    bottom_pos,
                    LinkType::Default,
                    0,
                    id,
                    true,
                );
                self.plot.redpiler.nodes[id.index].inputs = inputs;
            }
            Block::RedstoneWallTorch { facing, .. } => {
                let wall_pos = node.pos.offset(facing.opposite().block_face());
                let wall_block = self.plot.get_block(wall_pos);
                let inputs = self.get_redstone_links(
                    wall_block,
                    facing.opposite().block_face(),
                    wall_pos,
                    LinkType::Default,
                    0,
                    id,
                    true,
                );
                self.plot.redpiler.nodes[id.index].inputs = inputs;
            }
            Block::RedstoneComparator { comparator } => {
                let facing = comparator.facing;

                let mut inputs = self.search_diode_inputs(id, node.pos, facing);
                inputs.append(&mut self.search_comparator_side(id, node.pos, facing.rotate()));
                inputs.append(&mut self.search_comparator_side(id, node.pos, facing.rotate_ccw()));

                let input_pos = node.pos.offset(facing.block_face());
                let input_block = self.plot.get_block(input_pos);
                if input_block.has_comparator_override() {
                    inputs.push(Link::new(
                        LinkType::Default,
                        id,
                        0,
                        self.pos_map[&input_pos],
                    ));
                }

                self.plot.redpiler.nodes[id.index].inputs = inputs;
            }
            Block::RedstoneRepeater { repeater } => {
                let facing = repeater.facing;

                let mut inputs = self.search_diode_inputs(id, node.pos, facing);
                self.search_repeater_side(id, node.pos, facing.rotate())
                    .map(|l| inputs.push(l));
                self.search_repeater_side(id, node.pos, facing.rotate_ccw())
                    .map(|l| inputs.push(l));
                self.plot.redpiler.nodes[id.index].inputs = inputs;
            }
            Block::RedstoneWire { .. } => {
                let inputs = self.search_wire(id, node.pos, LinkType::Default, 0);
                self.plot.redpiler.nodes[id.index].inputs = inputs;
            }
            Block::RedstoneLamp { .. } => {
                let mut inputs = Vec::new();
                for face in &BlockFace::values() {
                    let neighbor_pos = node.pos.offset(*face);
                    let neighbor_block = self.plot.get_block(neighbor_pos);
                    let mut links = self.get_redstone_links(
                        neighbor_block,
                        *face,
                        neighbor_pos,
                        LinkType::Default,
                        0,
                        id,
                        true,
                    );
                    inputs.append(&mut links);
                }
                self.plot.redpiler.nodes[id.index].inputs = inputs;
            }
            block if block.has_comparator_override() => {
                self.plot.redpiler.nodes[id.index].comparator_output =
                    block.get_comparator_override(self.plot, node.pos);
            }
            _ => {}
        }
    }

    fn search(&mut self) {
        let nodes = self.plot.redpiler.nodes.clone();
        for (i, node) in nodes.into_iter().enumerate() {
            let id = NodeId { index: i };
            self.search_node(id, node);
        }

        // Stripping needs to be done here before any update

        for (id, node) in self.plot.redpiler.nodes.clone().into_iter().enumerate() {
            for input_node in node.inputs {
                self.plot.redpiler.nodes[input_node.end.index]
                    .updates
                    .push(NodeId { index: id });
            }
        }
    }
}

struct RPTickEntry {
    ticks_left: u32,
    tick_priority: TickPriority,
    node: NodeId,
}

#[derive(Default)]
pub struct CompilerOptions {
    pub use_worldedit: bool,
    pub optimize: bool,
}

impl CompilerOptions {
    pub fn parse(str: &str) -> CompilerOptions {
        let mut co: CompilerOptions = Default::default();
        let options = str.split_whitespace();
        for option in options {
            match option {
                "--worldedit" | "-w" => co.use_worldedit = true,
                "--optimize" | "-O" => co.optimize = true,
                // FIXME: use actual error handling
                _ => panic!("Unrecognized option: {}", option),
            }
        }
        co
    }
}

#[derive(Default)]
pub struct Compiler {
    pub is_active: bool,
    pub change_queue: Vec<(BlockPos, Block)>,
    nodes: Vec<Node>,
    to_be_ticked: Vec<RPTickEntry>,
    pos_map: HashMap<BlockPos, NodeId>,
}

impl Compiler {
    pub fn compile(
        plot: &mut Plot,
        options: CompilerOptions,
        first_pos: Option<BlockPos>,
        second_pos: Option<BlockPos>,
    ) {
        if plot.redpiler.is_active {
            plot.redpiler.reset();
        }

        let (first_pos, second_pos) = if options.use_worldedit {
            (first_pos.unwrap(), second_pos.unwrap())
        } else {
            // Get plot corners
            (
                BlockPos::new(plot.x * 256, 0, plot.z * 256),
                BlockPos::new((plot.x + 1) * 256 - 1, 255, (plot.z + 1) * 256 - 1),
            )
        };

        Compiler::identify_nodes(plot, first_pos, second_pos);
        InputSearch::new(plot).search();
        let compiler = &mut plot.redpiler;
        compiler.is_active = true;
        // dbg!(&compiler.nodes);
        // println!("{}", compiler);

        // TODO: Everything else
    }

    pub fn reset(&mut self) {
        self.nodes.clear();
        self.is_active = false;
    }

    pub fn on_use_block(&mut self, pos: BlockPos) {
        let node_id = self.pos_map[&pos];
        let node = self.nodes[node_id.index].clone();
        match node.state {
            Block::StoneButton { mut button } => {
                button.powered = !button.powered;
                self.set_node(node_id, Block::StoneButton { button }, true);
            }
            Block::Lever { mut lever } => {
                lever.powered = !lever.powered;
                self.set_node(node_id, Block::Lever { lever }, true);
            }
            _ => panic!("Tried to use a {:?}", node.state),
        }
    }

    fn schedule_tick(&mut self, node_id: NodeId, delay: u32, priority: TickPriority) {
        self.to_be_ticked.push(RPTickEntry {
            node: node_id,
            ticks_left: delay,
            tick_priority: priority,
        });
        self.to_be_ticked
            .sort_by_key(|e| (e.ticks_left, e.tick_priority.clone()));
    }

    fn pending_tick_at(&mut self, node: NodeId) -> bool {
        self.to_be_ticked.iter().any(|e| e.node == node)
    }

    fn set_node(&mut self, node: NodeId, new_block: Block, update: bool) {
        let node = &mut self.nodes[node.index];
        node.state = new_block;
        let pos = node.pos;
        if update {
            for update in node.updates.clone() {
                self.update_node(update);
            }
        }
        self.change_queue.push((pos, new_block));
    }

    fn comparator_should_be_powered(
        &mut self,
        mode: ComparatorMode,
        input_strength: u8,
        power_on_sides: u8,
    ) -> bool {
        if input_strength == 0 {
            false
        } else if input_strength > power_on_sides {
            true
        } else {
            power_on_sides == input_strength && mode == ComparatorMode::Compare
        }
    }

    fn calculate_comparator_output(
        &mut self,
        mode: ComparatorMode,
        input_strength: u8,
        power_on_sides: u8,
    ) -> u8 {
        if mode == ComparatorMode::Subtract {
            input_strength.saturating_sub(power_on_sides)
        } else if input_strength >= power_on_sides {
            input_strength
        } else {
            0
        }
    }

    fn update_node(&mut self, node_id: NodeId) {
        let node = self.nodes[node_id.index].clone();

        let mut input_power = 0;
        let mut side_input_power = 0;
        for link in &node.inputs {
            let power = match link.ty {
                LinkType::Default => &mut input_power,
                LinkType::Side => &mut side_input_power,
            };
            *power = (*power).max(
                self.nodes[link.end.index]
                    .get_output_power()
                    .saturating_sub(link.weight),
            );
        }

        match node.state {
            Block::RedstoneRepeater { mut repeater } => {
                let should_be_locked = side_input_power > 0;
                if !repeater.locked && should_be_locked {
                    repeater.locked = true;
                    self.set_node(node_id, Block::RedstoneRepeater { repeater }, false);
                } else if repeater.locked && !should_be_locked {
                    repeater.locked = false;
                    self.set_node(node_id, Block::RedstoneRepeater { repeater }, false);
                }

                if !repeater.locked && !self.pending_tick_at(node_id) {
                    let should_be_powered = input_power > 0;
                    if should_be_powered != repeater.powered {
                        let priority = if node.facing_diode {
                            TickPriority::Highest
                        } else if !should_be_powered {
                            TickPriority::Higher
                        } else {
                            TickPriority::High
                        };
                        self.schedule_tick(node_id, repeater.delay as u32, priority);
                    }
                }
            }
            Block::RedstoneTorch { lit } => {
                if lit == (input_power > 0) && !self.pending_tick_at(node_id) {
                    self.schedule_tick(node_id, 1, TickPriority::Normal);
                }
            }
            Block::RedstoneComparator { comparator } => {
                if self.pending_tick_at(node_id) {
                    return;
                }
                let output_power = self.calculate_comparator_output(
                    comparator.mode,
                    input_power,
                    side_input_power,
                );
                let old_strength = node.comparator_output;
                if output_power != old_strength
                    || comparator.powered
                        != self.comparator_should_be_powered(
                            comparator.mode,
                            input_power,
                            side_input_power,
                        )
                {
                    let priority = if node.facing_diode {
                        TickPriority::High
                    } else {
                        TickPriority::Normal
                    };
                    self.schedule_tick(node_id, 1, priority);
                }
            }
            Block::RedstoneWallTorch { lit, .. } => {
                if lit == (input_power > 0) && !self.pending_tick_at(node_id) {
                    self.schedule_tick(node_id, 1, TickPriority::Normal);
                }
            }
            Block::RedstoneLamp { lit } => {
                let should_be_lit = input_power > 0;
                if lit && !should_be_lit {
                    self.schedule_tick(node_id, 2, TickPriority::Normal);
                } else if !lit && should_be_lit {
                    self.set_node(node_id, Block::RedstoneLamp { lit: true }, false);
                }
            }
            Block::RedstoneWire { mut wire } => {
                if wire.power != input_power {
                    wire.power = input_power;
                    self.set_node(node_id, Block::RedstoneWire { wire }, true);
                }
            }
            _ => panic!("Node {:?} should not be updated!", node.state),
        }
    }

    pub fn tick(&mut self) {
        for pending in &mut self.to_be_ticked {
            pending.ticks_left = pending.ticks_left.saturating_sub(1);
        }
        while self.to_be_ticked.first().map(|e| e.ticks_left).unwrap_or(1) == 0 {
            let entry = self.to_be_ticked.remove(0);
            let node_id = entry.node;
            let node = self.nodes[node_id.index].clone();

            let mut input_power = 0u8;
            let mut side_input_power = 0u8;
            for link in &node.inputs {
                let power = match link.ty {
                    LinkType::Default => &mut input_power,
                    LinkType::Side => &mut side_input_power,
                };
                *power += self.nodes[link.end.index]
                    .get_output_power()
                    .saturating_sub(link.weight);
            }

            match node.state {
                Block::RedstoneRepeater { mut repeater } => {
                    if repeater.locked {
                        continue;
                    }

                    let should_be_powered = input_power > 0;
                    if repeater.powered && !should_be_powered {
                        repeater.powered = false;
                        self.set_node(node_id, Block::RedstoneRepeater { repeater }, true);
                    } else if !repeater.powered {
                        repeater.powered = true;
                        self.set_node(node_id, Block::RedstoneRepeater { repeater }, true);
                    }
                }
                Block::RedstoneTorch { lit } => {
                    let should_be_off = input_power > 0;
                    if lit && should_be_off {
                        self.set_node(node_id, Block::RedstoneTorch { lit: false }, true);
                    } else if !lit && !should_be_off {
                        self.set_node(node_id, Block::RedstoneTorch { lit: true }, true);
                    }
                }
                Block::RedstoneComparator { mut comparator } => {
                    let new_strength = self.calculate_comparator_output(
                        comparator.mode,
                        input_power,
                        side_input_power,
                    );
                    let old_strength = node.comparator_output;
                    if new_strength != old_strength || comparator.mode == ComparatorMode::Compare {
                        self.nodes[node_id.index].comparator_output = new_strength;
                        let should_be_powered = self.comparator_should_be_powered(
                            comparator.mode,
                            input_power,
                            side_input_power,
                        );
                        let powered = comparator.powered;
                        if powered && !should_be_powered {
                            comparator.powered = false;
                        } else if !powered && should_be_powered {
                            comparator.powered = true;
                        }
                        self.set_node(node_id, Block::RedstoneComparator { comparator }, true);
                    }
                }
                Block::RedstoneWallTorch { lit, facing } => {
                    let should_be_off = input_power > 0;
                    if lit && should_be_off {
                        self.set_node(
                            node_id,
                            Block::RedstoneWallTorch { lit: false, facing },
                            true,
                        );
                    } else if !lit && !should_be_off {
                        self.set_node(
                            node_id,
                            Block::RedstoneWallTorch { lit: true, facing },
                            true,
                        );
                    }
                }
                Block::RedstoneLamp { lit } => {
                    let should_be_lit = input_power > 0;
                    if lit && !should_be_lit {
                        self.set_node(node_id, Block::RedstoneLamp { lit: false }, false);
                    }
                }
                _ => panic!("Node {:?} should not be ticked!", node.state),
            }
        }
    }

    fn identify_node(&mut self, pos: BlockPos, block: Block, facing_diode: bool) {
        if let Some(node) = Node::from_block(pos, block, facing_diode) {
            self.nodes.push(node);
        }
    }

    fn identify_nodes(plot: &mut Plot, first_pos: BlockPos, second_pos: BlockPos) {
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
                    plot.redpiler.identify_node(pos, block, facing_diode);
                }
            }
        }
    }
}

impl Display for Compiler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("digraph{")?;
        for (id, node) in self.nodes.iter().enumerate() {
            write!(
                f,
                "n{}[label=\"{}\\n({}, {}, {})\"];",
                id,
                format!("{:?}", node.state)
                    .split_whitespace()
                    .next()
                    .unwrap(),
                node.pos.x,
                node.pos.y,
                node.pos.z
            )?;
            for link in &node.inputs {
                let color = match link.ty {
                    LinkType::Default => "",
                    LinkType::Side => ",color=\"blue\"",
                };
                write!(
                    f,
                    "n{}->n{}[label=\"{}\"{}];",
                    link.end.index, link.start.index, link.weight, color
                )?;
            }
            // for update in &node.updates {
            //     write!(f, "n{}->n{}[style=dotted];", id, update.index)?;
            // }
        }
        f.write_str("}\n")
    }
}
