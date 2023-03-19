use crate::blocks::{
    ActionResult, Block, BlockDirection, BlockFace, BlockPos, BlockProperty, BlockTransform,
    FlipDirection,
};
use crate::world::World;
use std::collections::HashMap;
use std::str::FromStr;

impl Block {
    fn unwrap_wire(self) -> RedstoneWire {
        match self {
            Block::RedstoneWire { wire } => wire,
            _ => panic!("expected wire"),
        }
    }

    fn wire_mut(&mut self) -> &mut RedstoneWire {
        match self {
            Block::RedstoneWire { wire } => wire,
            _ => panic!("expected wire"),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum RedstoneWireSide {
    Up,
    Side,
    #[default]
    None,
}

impl RedstoneWireSide {
    pub fn is_none(self) -> bool {
        matches!(self, RedstoneWireSide::None)
    }
}

impl FromStr for RedstoneWireSide {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "up" => RedstoneWireSide::Up,
            "side" => RedstoneWireSide::Side,
            "none" => RedstoneWireSide::None,
            _ => return Err(()),
        })
    }
}

impl ToString for RedstoneWireSide {
    fn to_string(&self) -> String {
        match self {
            RedstoneWireSide::Up => "up".to_owned(),
            RedstoneWireSide::Side => "side".to_owned(),
            RedstoneWireSide::None => "none".to_owned(),
        }
    }
}

impl RedstoneWireSide {
    pub fn from_id(id: u32) -> RedstoneWireSide {
        match id {
            0 => RedstoneWireSide::Up,
            1 => RedstoneWireSide::Side,
            2 => RedstoneWireSide::None,
            _ => panic!("Invalid RedstoneWireSide"),
        }
    }

    pub fn get_id(self) -> u32 {
        match self {
            RedstoneWireSide::Up => 0,
            RedstoneWireSide::Side => 1,
            RedstoneWireSide::None => 2,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, BlockProperty)]
pub struct RedstoneWire {
    pub north: RedstoneWireSide,
    pub south: RedstoneWireSide,
    pub east: RedstoneWireSide,
    pub west: RedstoneWireSide,
    pub power: u8,
}

impl RedstoneWire {
    const CROSS: RedstoneWire = RedstoneWire {
        north: RedstoneWireSide::Side,
        south: RedstoneWireSide::Side,
        east: RedstoneWireSide::Side,
        west: RedstoneWireSide::Side,
        power: 0,
    };

    pub fn new(
        north: RedstoneWireSide,
        south: RedstoneWireSide,
        east: RedstoneWireSide,
        west: RedstoneWireSide,
        power: u8,
    ) -> RedstoneWire {
        RedstoneWire {
            north,
            south,
            east,
            west,
            power,
        }
    }

    pub fn get_state_for_placement(world: &impl World, pos: BlockPos) -> RedstoneWire {
        let mut wire = RedstoneWire {
            power: RedstoneWire::calculate_power(world, pos),
            ..Default::default()
        };
        wire = wire.get_regulated_sides(world, pos);
        if wire.is_dot() {
            let mut cross = RedstoneWire::CROSS;
            cross.power = wire.power;
            wire = cross;
        }
        wire
    }

    pub fn on_neighbor_changed(
        mut self,
        world: &impl World,
        pos: BlockPos,
        side: BlockFace,
    ) -> RedstoneWire {
        let old_state = self;
        let new_side;
        match side {
            BlockFace::Top => return self,
            BlockFace::Bottom => {
                return self.get_regulated_sides(world, pos);
            }
            BlockFace::North => {
                self.south = RedstoneWire::get_side(world, pos, BlockDirection::South);
                new_side = self.south;
            }
            BlockFace::South => {
                self.north = RedstoneWire::get_side(world, pos, BlockDirection::North);
                new_side = self.north;
            }

            BlockFace::East => {
                self.west = RedstoneWire::get_side(world, pos, BlockDirection::West);
                new_side = self.west;
            }
            BlockFace::West => {
                self.east = RedstoneWire::get_side(world, pos, BlockDirection::East);
                new_side = self.east;
            }
        }
        self = self.get_regulated_sides(world, pos);
        if old_state.is_cross() && new_side.is_none() {
            // Don't mess up the cross
            return old_state;
        }
        if !old_state.is_dot() && self.is_dot() {
            // Save the power until the transformation into cross is complete
            let power = self.power;
            // Become the cross it always wanted to be
            self = RedstoneWire::CROSS;
            self.power = power;
        }
        self
    }

    pub fn on_neighbor_updated(mut self, world: &mut impl World, pos: BlockPos) {
        let new_power = RedstoneWire::calculate_power(world, pos);

        if self.power != new_power {
            self.power = new_power;
            world.set_block(pos, Block::RedstoneWire { wire: self });
            RedstoneWireTurbo::update_surrounding_neighbors(world, pos);
        }
    }

    pub fn on_use(self, world: &mut impl World, pos: BlockPos) -> ActionResult {
        if self.is_dot() || self.is_cross() {
            let mut new_wire = if self.is_cross() {
                RedstoneWire::default()
            } else {
                RedstoneWire::CROSS
            };
            new_wire.power = self.power;
            new_wire = new_wire.get_regulated_sides(world, pos);
            if self != new_wire {
                world.set_block(pos, Block::RedstoneWire { wire: new_wire });
                Block::update_wire_neighbors(world, pos);
                return ActionResult::Success;
            }
        }
        ActionResult::Pass
    }

    fn can_connect_to(block: Block, side: BlockDirection) -> bool {
        match block {
            Block::RedstoneWire { .. }
            | Block::RedstoneComparator { .. }
            | Block::RedstoneTorch { .. }
            | Block::RedstoneBlock { .. }
            | Block::RedstoneWallTorch { .. }
            | Block::StonePressurePlate { .. }
            | Block::TripwireHook { .. }
            | Block::StoneButton { .. }
            | Block::Target { .. }
            | Block::Lever { .. } => true,
            Block::RedstoneRepeater { repeater } => {
                repeater.facing == side || repeater.facing == side.opposite()
            }
            Block::Observer { facing } => facing == side.block_facing(),
            _ => false,
        }
    }

    fn can_connect_diagonal_to(block: Block) -> bool {
        matches!(block, Block::RedstoneWire { .. })
    }

    pub fn get_current_side(self, side: BlockDirection) -> RedstoneWireSide {
        use BlockDirection::*;
        match side {
            North => self.north,
            South => self.south,
            East => self.east,
            West => self.west,
        }
    }

    pub fn get_side(world: &impl World, pos: BlockPos, side: BlockDirection) -> RedstoneWireSide {
        let neighbor_pos = pos.offset(side.block_face());
        let neighbor = world.get_block(neighbor_pos);

        if RedstoneWire::can_connect_to(neighbor, side) {
            return RedstoneWireSide::Side;
        }

        let up_pos = pos.offset(BlockFace::Top);
        let up = world.get_block(up_pos);

        if !up.is_solid()
            && RedstoneWire::can_connect_diagonal_to(
                world.get_block(neighbor_pos.offset(BlockFace::Top)),
            )
        {
            RedstoneWireSide::Up
        } else if !neighbor.is_solid()
            && RedstoneWire::can_connect_diagonal_to(
                world.get_block(neighbor_pos.offset(BlockFace::Bottom)),
            )
        {
            RedstoneWireSide::Side
        } else {
            RedstoneWireSide::None
        }
    }

    fn get_all_sides(mut self, world: &impl World, pos: BlockPos) -> RedstoneWire {
        self.north = Self::get_side(world, pos, BlockDirection::North);
        self.south = Self::get_side(world, pos, BlockDirection::South);
        self.east = Self::get_side(world, pos, BlockDirection::East);
        self.west = Self::get_side(world, pos, BlockDirection::West);
        self
    }

    pub fn get_regulated_sides(self, world: &impl World, pos: BlockPos) -> RedstoneWire {
        let is_dot = self.is_dot();
        let mut state = self.get_all_sides(world, pos);
        if is_dot && state.is_dot() {
            return state;
        }
        let north_none = state.north.is_none();
        let south_none = state.south.is_none();
        let east_none = state.east.is_none();
        let west_none = state.west.is_none();
        let north_south_none = north_none && south_none;
        let east_west_none = east_none && west_none;
        if north_none && east_west_none {
            state.north = RedstoneWireSide::Side;
        }
        if south_none && east_west_none {
            state.south = RedstoneWireSide::Side;
        }
        if east_none && north_south_none {
            state.east = RedstoneWireSide::Side;
        }
        if west_none && north_south_none {
            state.west = RedstoneWireSide::Side;
        }
        state
    }

    fn is_dot(self) -> bool {
        self.north == RedstoneWireSide::None
            && self.south == RedstoneWireSide::None
            && self.east == RedstoneWireSide::None
            && self.west == RedstoneWireSide::None
    }

    fn is_cross(self) -> bool {
        self.north == RedstoneWireSide::Side
            && self.south == RedstoneWireSide::Side
            && self.east == RedstoneWireSide::Side
            && self.west == RedstoneWireSide::Side
    }

    fn max_wire_power(wire_power: u8, world: &impl World, pos: BlockPos) -> u8 {
        let block = world.get_block(pos);
        if let Block::RedstoneWire { wire } = block {
            wire_power.max(wire.power)
        } else {
            wire_power
        }
    }

    fn calculate_power(world: &impl World, pos: BlockPos) -> u8 {
        let mut block_power = 0;
        let mut wire_power = 0;

        let up_pos = pos.offset(BlockFace::Top);
        let up_block = world.get_block(up_pos);

        for side in &BlockFace::values() {
            let neighbor_pos = pos.offset(*side);
            wire_power = RedstoneWire::max_wire_power(wire_power, world, neighbor_pos);
            let neighbor = world.get_block(neighbor_pos);
            block_power =
                block_power.max(neighbor.get_redstone_power_no_dust(world, neighbor_pos, *side));
            if side.is_horizontal() {
                if !up_block.is_solid() && !neighbor.is_transparent() {
                    wire_power = RedstoneWire::max_wire_power(
                        wire_power,
                        world,
                        neighbor_pos.offset(BlockFace::Top),
                    );
                }

                if !neighbor.is_solid() {
                    wire_power = RedstoneWire::max_wire_power(
                        wire_power,
                        world,
                        neighbor_pos.offset(BlockFace::Bottom),
                    );
                }
            }
        }

        block_power.max(wire_power.saturating_sub(1))
    }
}

impl BlockTransform for RedstoneWire {
    fn rotate90(&mut self) {
        *self = RedstoneWire {
            north: self.west,
            east: self.north,
            south: self.east,
            west: self.south,
            ..*self
        }
    }

    fn flip(&mut self, dir: FlipDirection) {
        *self = match dir {
            FlipDirection::FlipX => RedstoneWire {
                east: self.west,
                west: self.east,
                ..*self
            },
            FlipDirection::FlipZ => RedstoneWire {
                north: self.south,
                south: self.north,
                ..*self
            },
        }
    }
}

#[derive(Clone, Copy)]
struct NodeId {
    index: usize,
}

struct UpdateNode {
    pos: BlockPos,
    /// The cached state of the block
    state: Block,
    /// This will only be `Some` when all the neighbors are identified.
    neighbors: Option<Vec<NodeId>>,
    visited: bool,
    xbias: i32,
    zbias: i32,
    layer: u32,
}

impl UpdateNode {
    fn new(world: &impl World, pos: BlockPos) -> UpdateNode {
        UpdateNode {
            pos,
            state: world.get_block(pos),
            visited: false,
            neighbors: None,
            xbias: 0,
            zbias: 0,
            layer: 0,
        }
    }
}

/// The implementation of "Redstone Wire Turbo" was largely based on
/// the accelorator created by theosib. For more information, see:
/// https://bugs.mojang.com/browse/MC-81098.
struct RedstoneWireTurbo {
    nodes: Vec<UpdateNode>,
    node_cache: HashMap<BlockPos, NodeId>,
    update_queue: Vec<Vec<NodeId>>,
    current_walk_layer: u32,
}

impl RedstoneWireTurbo {
    // Internal numbering for cardinal directions
    const NORTH: usize = 0;
    const EAST: usize = 1;
    const SOUTH: usize = 2;
    const WEST: usize = 3;

    fn new() -> RedstoneWireTurbo {
        RedstoneWireTurbo {
            nodes: Vec::new(),
            node_cache: HashMap::new(),
            update_queue: vec![vec![], vec![], vec![]],
            current_walk_layer: 0,
        }
    }

    fn get_node(&self, node_id: NodeId) -> &UpdateNode {
        &self.nodes[node_id.index]
    }

    fn compute_all_neighbors(pos: BlockPos) -> [BlockPos; 24] {
        let BlockPos { x, y, z } = pos;
        [
            BlockPos::new(x - 1, y, z),
            BlockPos::new(x + 1, y, z),
            BlockPos::new(x, y - 1, z),
            BlockPos::new(x, y + 1, z),
            BlockPos::new(x, y, z - 1),
            BlockPos::new(x, y, z + 1),
            // Neighbors of neighbors, in the same order,
            // except that duplicates are not included
            BlockPos::new(x - 2, y, z),
            BlockPos::new(x - 1, y - 1, z),
            BlockPos::new(x - 1, y + 1, z),
            BlockPos::new(x - 1, y, z - 1),
            BlockPos::new(x - 1, y, z + 1),
            BlockPos::new(x + 2, y, z),
            BlockPos::new(x + 1, y - 1, z),
            BlockPos::new(x + 1, y + 1, z),
            BlockPos::new(x + 1, y, z - 1),
            BlockPos::new(x + 1, y, z + 1),
            BlockPos::new(x, y - 2, z),
            BlockPos::new(x, y - 1, z - 1),
            BlockPos::new(x, y - 1, z + 1),
            BlockPos::new(x, y + 2, z),
            BlockPos::new(x, y + 1, z - 1),
            BlockPos::new(x, y + 1, z + 1),
            BlockPos::new(x, y, z - 2),
            BlockPos::new(x, y, z + 2),
        ]
    }

    fn compute_heading(rx: i32, rz: i32) -> usize {
        let code = (rx + 1) + 3 * (rz + 1);
        match code {
            0 => Self::NORTH,
            1 => Self::NORTH,
            2 => Self::EAST,
            3 => Self::WEST,
            4 => Self::WEST,
            5 => Self::EAST,
            6 => Self::SOUTH,
            7 => Self::SOUTH,
            8 => Self::SOUTH,
            _ => unreachable!(),
        }
    }

    // const UPDATE_REDSTONE: [bool; 24] = [
    //     true, true, false, false, true, true,   // 0 to 5
    //     false, true, true, false, false, false, // 6 to 11
    //     true, true, false, false, false, true,  // 12 to 17
    //     true, false, true, true, false, false   // 18 to 23
    // ];

    fn identify_neighbors(&mut self, world: &mut impl World, upd1: NodeId) {
        let pos = self.nodes[upd1.index].pos;
        let neighbors = Self::compute_all_neighbors(pos);
        let mut neighbors_visited = Vec::with_capacity(24);
        let mut neighbor_nodes = Vec::with_capacity(24);

        for (_i, neighbor_pos) in neighbors[0..24].iter().enumerate() {
            let neighbor = if !self.node_cache.contains_key(neighbor_pos) {
                let node_id = NodeId {
                    index: self.nodes.len(),
                };
                self.node_cache.insert(*neighbor_pos, node_id);
                self.nodes.push(UpdateNode::new(world, *neighbor_pos));
                node_id
            } else {
                self.node_cache[neighbor_pos]
            };

            let node = &self.nodes[neighbor.index];
            // if let Block::RedstoneWire { .. } = node.state {
            //     if RedstoneWireTurbo::UPDATE_REDSTONE[i] {
            neighbor_nodes.push(neighbor);
            neighbors_visited.push(node.visited);
            //         continue;
            //     }
            // }
            // neighbor_nodes.push(None);
            // neighbors_visited.push(false);
        }

        let from_west = neighbors_visited[0] || neighbors_visited[7] || neighbors_visited[8];
        let from_east = neighbors_visited[1] || neighbors_visited[12] || neighbors_visited[13];
        let from_north = neighbors_visited[4] || neighbors_visited[17] || neighbors_visited[20];
        let from_south = neighbors_visited[5] || neighbors_visited[18] || neighbors_visited[21];

        let mut cx = 0;
        let mut cz = 0;
        if from_west {
            cx += 1;
        };
        if from_east {
            cx -= 1;
        };
        if from_north {
            cz += 1;
        };
        if from_south {
            cz -= 1;
        };

        let UpdateNode { xbias, zbias, .. } = &self.nodes[upd1.index];
        let xbias = *xbias;
        let zbias = *zbias;

        let heading;
        if cx == 0 && cz == 0 {
            heading = Self::compute_heading(xbias, zbias);

            for node_id in &neighbor_nodes {
                // if let Some(node_id) = node_id {
                let nn = &mut self.nodes[node_id.index];
                nn.xbias = xbias;
                nn.zbias = zbias;
                // }
            }
        } else {
            if cx != 0 && cz != 0 {
                if xbias != 0 {
                    cz = 0;
                }
                if zbias != 0 {
                    cx = 0;
                }
            }
            heading = Self::compute_heading(cx, cz);

            for node_id in &neighbor_nodes {
                // if let Some(node_id) = node_id {
                let nn = &mut self.nodes[node_id.index];
                nn.xbias = cx;
                nn.zbias = cz;
                // }
            }
        }

        self.orient_neighbors(&neighbor_nodes, upd1, heading);
    }

    const REORDING: [[usize; 24]; 4] = [
        [
            2, 3, 16, 19, 0, 4, 1, 5, 7, 8, 17, 20, 12, 13, 18, 21, 6, 9, 22, 14, 11, 10, 23, 15,
        ],
        [
            2, 3, 16, 19, 4, 1, 5, 0, 17, 20, 12, 13, 18, 21, 7, 8, 22, 14, 11, 15, 23, 9, 6, 10,
        ],
        [
            2, 3, 16, 19, 1, 5, 0, 4, 12, 13, 18, 21, 7, 8, 17, 20, 11, 15, 23, 10, 6, 14, 22, 9,
        ],
        [
            2, 3, 16, 19, 5, 0, 4, 1, 18, 21, 7, 8, 17, 20, 12, 13, 23, 10, 6, 9, 22, 15, 11, 14,
        ],
    ];

    fn orient_neighbors(&mut self, src: &[NodeId], dst_id: NodeId, heading: usize) {
        let dst = &mut self.nodes[dst_id.index];
        let mut neighbors = Vec::with_capacity(24);
        let re = Self::REORDING[heading];
        for i in &re {
            neighbors.push(src[*i]);
        }
        dst.neighbors = Some(neighbors);
    }

    /// This is the start of a great adventure
    fn update_surrounding_neighbors(world: &mut impl World, pos: BlockPos) {
        let mut turbo = RedstoneWireTurbo::new();
        let mut root_node = UpdateNode::new(world, pos);
        root_node.visited = true;
        let node_id = NodeId { index: 0 };
        turbo.node_cache.insert(pos, node_id);
        turbo.nodes.push(root_node);
        turbo.propagate_changes(world, node_id, 0);
        turbo.breadth_first_walk(world);
    }

    fn propagate_changes(&mut self, world: &mut impl World, upd1: NodeId, layer: u32) {
        if self.nodes[upd1.index].neighbors.is_none() {
            self.identify_neighbors(world, upd1);
        }
        // FIXME: Get rid of this nasty clone
        let neighbors = self.nodes[upd1.index].neighbors.clone().unwrap();

        let layer1 = layer + 1;

        for neighbor_id in &neighbors[0..24] {
            let neighbor = &mut self.nodes[neighbor_id.index];
            if layer1 > neighbor.layer {
                neighbor.layer = layer1;
                self.update_queue[1].push(*neighbor_id);
            }
        }

        let layer2 = layer + 2;

        for neighbor_id in &neighbors[0..4] {
            let neighbor = &mut self.nodes[neighbor_id.index];
            if layer2 > neighbor.layer {
                neighbor.layer = layer2;
                self.update_queue[2].push(*neighbor_id);
            }
        }
    }

    fn breadth_first_walk(&mut self, world: &mut impl World) {
        self.shift_queue();
        self.current_walk_layer = 1;

        while !self.update_queue[0].is_empty() || !self.update_queue[1].is_empty() {
            for node_id in self.update_queue[0].clone() {
                match self.nodes[node_id.index].state {
                    Block::RedstoneWire { .. } => {
                        self.update_node(world, node_id, self.current_walk_layer);
                    }
                    // This only works because updating any other block than a wire will
                    // never change the state of the block. If that changes in the future,
                    // the cached state will need to be updated
                    block => Block::update(block, world, self.nodes[node_id.index].pos),
                }
            }

            self.shift_queue();
            self.current_walk_layer += 1;
        }

        self.current_walk_layer = 0;
    }

    fn shift_queue(&mut self) {
        let mut t = self.update_queue.remove(0);
        t.clear();
        self.update_queue.push(t);
    }

    fn update_node(&mut self, world: &mut impl World, upd1: NodeId, layer: u32) {
        let old_wire = {
            let node = &mut self.nodes[upd1.index];
            node.visited = true;
            node.state.unwrap_wire()
        };

        let new_wire = self.calculate_current_changes(world, upd1);
        if old_wire.power != new_wire.power {
            self.nodes[upd1.index].state.wire_mut().power = new_wire.power;

            self.propagate_changes(world, upd1, layer);
        }
    }

    const RS_NEIGHBORS: [usize; 4] = [4, 5, 6, 7];
    const RS_NEIGHBORS_UP: [usize; 4] = [9, 11, 13, 15];
    const RS_NEIGHBORS_DN: [usize; 4] = [8, 10, 12, 14];

    fn calculate_current_changes(&mut self, world: &mut impl World, upd: NodeId) -> RedstoneWire {
        let mut wire = self.nodes[upd.index].state.unwrap_wire();
        let i = wire.power;
        let mut block_power = 0;

        if self.nodes[upd.index].neighbors.is_none() {
            self.identify_neighbors(world, upd);
        }

        let pos = self.nodes[upd.index].pos;

        let mut wire_power = 0;
        for side in &BlockFace::values() {
            let neighbor_pos = pos.offset(*side);
            let neighbor = self.nodes[self.node_cache[&neighbor_pos].index].state;
            wire_power =
                wire_power.max(neighbor.get_redstone_power_no_dust(world, neighbor_pos, *side));
        }

        if wire_power < 15 {
            let neighbors = self.nodes[upd.index].neighbors.as_ref().unwrap();

            let center_up = self.nodes[neighbors[1].index].state;

            for m in 0..4 {
                let n = Self::RS_NEIGHBORS[m];

                let neighbor_id = neighbors[n];
                let neighbor = self.get_node(neighbor_id).state;
                block_power = self.get_max_current_strength(neighbor_id, block_power);

                if !neighbor.is_solid() {
                    let neighbor_down = neighbors[Self::RS_NEIGHBORS_DN[m]];
                    block_power = self.get_max_current_strength(neighbor_down, block_power);
                } else if !center_up.is_solid() && !neighbor.is_transparent() {
                    let neighbor_up = neighbors[Self::RS_NEIGHBORS_UP[m]];
                    block_power = self.get_max_current_strength(neighbor_up, block_power);
                }
            }
        }

        let mut j = block_power.saturating_sub(1);
        if wire_power > j {
            j = wire_power;
        }
        if i != j {
            wire.power = j;
            world.set_block(pos, Block::RedstoneWire { wire });
        }
        wire
    }

    fn get_max_current_strength(&self, upd: NodeId, strength: u8) -> u8 {
        let node = &self.nodes[upd.index];
        if let Block::RedstoneWire { wire } = node.state {
            wire.power.max(strength)
        } else {
            strength
        }
    }
}
