use crate::blocks::{ActionResult, Block, BlockDirection, BlockFace, BlockPos};
use crate::world::World;
use std::collections::HashMap;

// Redstone wires are extremely inefficient.
// Here we are updating many blocks which don't
// need to be updated. A lot of the time we even
// updating the same redstone wire twice. In the
// future we can use the algorithm created by
// theosib to greatly speed this up.
// The comments in this issue might be useful:
// https://bugs.mojang.com/browse/MC-81098

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum RedstoneWireSide {
    Up,
    Side,
    None,
}

impl RedstoneWireSide {
    pub fn is_none(self) -> bool {
        match self {
            RedstoneWireSide::None => true,
            _ => false,
        }
    }

    pub fn from_str(name: &str) -> RedstoneWireSide {
        match name {
            "up" => RedstoneWireSide::Up,
            "side" => RedstoneWireSide::Side,
            _ => RedstoneWireSide::None,
        }
    }
}

impl Default for RedstoneWireSide {
    fn default() -> RedstoneWireSide {
        RedstoneWireSide::None
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

#[derive(Copy, Clone, Debug, PartialEq, Default)]
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

    pub fn get_state_for_placement(world: &dyn World, pos: BlockPos) -> RedstoneWire {
        let mut wire = RedstoneWire::default();
        wire.power = RedstoneWire::calculate_power(world, pos);
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
        world: &dyn World,
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
                new_side = self.west
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

    pub fn on_neighbor_updated(mut self, world: &mut dyn World, pos: BlockPos) {
        let new_power = RedstoneWire::calculate_power(world, pos);

        if self.power != new_power {
            self.power = new_power;
            world.set_block(pos, Block::RedstoneWire { wire: self });
            RedstoneWireTurbo::update_surrounding_neighbors(world, pos);
        }
    }

    pub fn on_use(self, world: &mut dyn World, pos: BlockPos) -> ActionResult {
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
        match block {
            Block::RedstoneWire { .. } => true,
            _ => false,
        }
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

    pub fn get_side(world: &dyn World, pos: BlockPos, side: BlockDirection) -> RedstoneWireSide {
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

    fn get_all_sides(mut self, world: &dyn World, pos: BlockPos) -> RedstoneWire {
        self.north = Self::get_side(world, pos, BlockDirection::North);
        self.south = Self::get_side(world, pos, BlockDirection::South);
        self.east = Self::get_side(world, pos, BlockDirection::East);
        self.west = Self::get_side(world, pos, BlockDirection::West);
        self
    }

    pub fn get_regulated_sides(self, world: &dyn World, pos: BlockPos) -> RedstoneWire {
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

    fn max_wire_power(wire_power: u8, world: &dyn World, pos: BlockPos) -> u8 {
        let block = world.get_block(pos);
        if let Block::RedstoneWire { wire } = block {
            wire_power.max(wire.power)
        } else {
            wire_power
        }
    }

    fn calculate_power(world: &dyn World, pos: BlockPos) -> u8 {
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

struct UpdateNode {
    pos: BlockPos,
    parent: BlockPos,
    /// If the node is a redstone wire, it will hold the state of the wire
    wire: Option<RedstoneWire>,
    /// This will only be `Some` when all the neighbors are identified.
    neighbors: Option<Vec<BlockPos>>,
    visited: bool,
    xbias: i32,
    zbias: i32,
    layer: u32,
}

impl UpdateNode {
    fn new(world: &dyn World, pos: BlockPos, parent: BlockPos) -> UpdateNode {
        UpdateNode {
            pos,
            parent,
            wire: match world.get_block(pos) {
                Block::RedstoneWire { wire } => Some(wire),
                _ => None,
            },
            visited: false,
            neighbors: None,
            xbias: 0,
            zbias: 0,
            layer: 0,
        }
    }
}

struct RedstoneWireTurbo {
    node_cache: HashMap<BlockPos, UpdateNode>,
    update_queue: Vec<Vec<BlockPos>>,
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
            node_cache: HashMap::new(),
            update_queue: vec![vec![], vec![], vec![]],
            current_walk_layer: 0,
        }
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

    fn identify_neighbors(&mut self, world: &mut dyn World, pos: BlockPos) {
        let neighbors = Self::compute_all_neighbors(pos);
        let mut neighbors_visited = Vec::with_capacity(24);

        for i in 0..24 {
            let neighbor_pos = neighbors[i];
            let neighbor = self
                .node_cache
                .entry(neighbor_pos)
                .or_insert_with(|| UpdateNode::new(world, neighbor_pos, pos));
            neighbors_visited.push(neighbor.visited);
        }

        let fromWest = neighbors_visited[0] || neighbors_visited[7] || neighbors_visited[8];
        let fromEast = neighbors_visited[1] || neighbors_visited[12] || neighbors_visited[13];
        let fromNorth = neighbors_visited[4] || neighbors_visited[17] || neighbors_visited[20];
        let fromSouth = neighbors_visited[5] || neighbors_visited[18] || neighbors_visited[21];

        let mut cx = 0;
        let mut cz = 0;
        if fromWest {
            cx += 1
        };
        if fromEast {
            cx -= 1
        };
        if fromNorth {
            cz += 1
        };
        if fromSouth {
            cz -= 1
        };

        let UpdateNode { xbias, zbias, .. } = self.node_cache.get(&pos).unwrap();
        let xbias = *xbias;
        let zbias = *zbias;

        let heading;
        if cx == 0 && cz == 0 {
            heading = Self::compute_heading(xbias, zbias);

            for i in &neighbors {
                if let Some(nn) = self.node_cache.get_mut(i) {
                    nn.xbias = xbias;
                    nn.zbias = zbias;
                }
            }
        } else {
            if cx != 0 && cz != 0 {
                if xbias != 0 {
                    cz = 0
                }
                if zbias != 0 {
                    cx = 0
                }
            }
            heading = Self::compute_heading(cx, cz);

            for i in &neighbors {
                if let Some(nn) = self.node_cache.get_mut(i) {
                    nn.xbias = cx;
                    nn.zbias = cz;
                }
            }
        }

        self.orient_neighbors(neighbors, pos, heading);
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

    fn orient_neighbors(&mut self, src: [BlockPos; 24], dst_pos: BlockPos, heading: usize) {
        let dst = self.node_cache.get_mut(&dst_pos).unwrap();
        let mut neighbors = Vec::with_capacity(24);
        let re = Self::REORDING[heading];
        for i in &re {
            neighbors.push(src[*i]);
        }
        dst.neighbors = Some(neighbors);
    }

    /// This is the start of a great adventure
    fn update_surrounding_neighbors(world: &mut dyn World, pos: BlockPos) {
        let mut turbo = RedstoneWireTurbo::new();
        let mut root_node = UpdateNode::new(world, pos, pos);
        root_node.visited = true;
        turbo.node_cache.insert(pos, root_node);
        turbo.propogate_changes(world, pos, 0);
        turbo.breadth_first_walk(world);
    }

    fn propogate_changes(&mut self, world: &mut dyn World, pos: BlockPos, layer: u32) {
        if self.node_cache.get(&pos).unwrap().neighbors.is_none() {
            self.identify_neighbors(world, pos);
        }
        let BlockPos { x, y, z } = pos;
        // FIXME: Get rid of this nasty clone
        let neighbors = self
            .node_cache
            .get(&pos)
            .unwrap()
            .neighbors
            .clone()
            .unwrap();

        let layer1 = layer + 1;

        for i in 0..24 {
            let neighbor_pos = neighbors[i];
            if let Some(neighbor) = self.node_cache.get_mut(&neighbor_pos) {
                if layer1 > neighbor.layer {
                    neighbor.layer = layer1;
                    self.update_queue[1].push(neighbor_pos);

                    neighbor.parent = pos;
                }
            }
        }

        let layer2 = layer + 2;

        for i in 0..4 {
            let neighbor_pos = neighbors[i];
            if let Some(neighbor) = self.node_cache.get_mut(&neighbor_pos) {
                if layer2 > neighbor.layer {
                    neighbor.layer = layer2;
                    self.update_queue[2].push(neighbor_pos);
                    neighbor.parent = pos;
                }
            }
        }
    }

    fn breadth_first_walk(&mut self, world: &mut dyn World) {
        self.shift_queue();
        self.current_walk_layer = 1;

        while !self.update_queue[0].is_empty() || !self.update_queue[1].is_empty() {
            for pos in self.update_queue[0].clone().into_iter() {
                if self.node_cache.get(&pos).unwrap().wire.is_some() {
                    self.update_node(world, pos, self.current_walk_layer);
                } else {
                    let block = world.get_block(pos);
                    Block::update(block, world, pos);
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

    fn update_node(&mut self, world: &mut dyn World, pos: BlockPos, layer: u32) {
        let old_wire = {
            let node = self.node_cache.get_mut(&pos).unwrap();
            node.visited = true;
            node.wire.unwrap()
        };

        let new_wire = self.calculate_current_changes(world, pos);
        if old_wire.power != new_wire.power {
            self.node_cache
                .get_mut(&pos)
                .unwrap()
                .wire
                .as_mut()
                .unwrap()
                .power = new_wire.power;

            self.propogate_changes(world, pos, layer);
        }
    }

    const RS_NEIGHBORS: [usize; 4] = [4, 5, 6, 7];
    const RS_NEIGHBORS_UP: [usize; 4] = [9, 11, 13, 15];
    const RS_NEIGHBORS_DN: [usize; 4] = [8, 10, 12, 14];

    fn calculate_current_changes(&mut self, world: &mut dyn World, pos: BlockPos) -> RedstoneWire {
        let mut wire = self.node_cache.get(&pos).unwrap().wire.unwrap();
        let i = wire.power;
        let mut j = self.get_max_current_strength(pos, 0);
        let mut l = 0;

        let mut k = 0;
        for side in &BlockFace::values() {
            // TODO: Use the accelerator caching to calculate this
            let neighbor_pos = pos.offset(*side);
            let neighbor = world.get_block(neighbor_pos);
            k = k.max(neighbor.get_redstone_power_no_dust(world, neighbor_pos, *side));
        }

        if k < 15 {
            if self.node_cache.get(&pos).unwrap().neighbors.is_none() {
                self.identify_neighbors(world, pos);
            }
            let neighbors = self
                .node_cache
                .get(&pos)
                .unwrap()
                .neighbors
                .as_ref()
                .unwrap();

            let center_up = world.get_block(neighbors[1]);

            for m in 0..4 {
                let n = Self::RS_NEIGHBORS[m];

                let neighbor_pos = neighbors[n];
                let neighbor = world.get_block(neighbor_pos);
                l = self.get_max_current_strength(neighbor_pos, l);

                if !neighbor.is_solid() {
                    let neighbor_down = neighbors[Self::RS_NEIGHBORS_DN[m]];
                    l = self.get_max_current_strength(neighbor_down, l);
                } else if !center_up.is_solid() && !neighbor.is_transparent() {
                    let neighbor_up = neighbors[Self::RS_NEIGHBORS_UP[m]];
                    l = self.get_max_current_strength(neighbor_up, l);
                }
            }
        }

        j = l.saturating_sub(1);
        if k > j {
            j = k;
        }
        if i != j {
            wire.power = j;
            world.set_block(pos, Block::RedstoneWire { wire });
        }
        wire
    }

    fn get_max_current_strength(&self, pos: BlockPos, strength: u8) -> u8 {
        let node = self.node_cache.get(&pos).unwrap();
        if let Some(wire) = node.wire {
            wire.power.max(strength)
        } else {
            strength
        }
    }
}
