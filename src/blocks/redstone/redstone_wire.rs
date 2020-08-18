use crate::blocks::{Block, BlockDirection, BlockFace, BlockPos, ActionResult};
use crate::world::World;

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
        power: 0
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
            },
            BlockFace::West => {
                self.east = RedstoneWire::get_side(world, pos, BlockDirection::East);
                new_side = self.east;
            },
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
            world.set_block(pos, Block::RedstoneWire(self));

            Block::update_wire_neighbors(world, pos);
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
                world.set_block(pos, Block::RedstoneWire(new_wire));
                Block::update_wire_neighbors(world, pos);
                return ActionResult::Success;
            }
        }
        ActionResult::Pass
    }

    fn can_connect_to(block: Block, side: BlockDirection) -> bool {
        match block {
            Block::RedstoneWire(_)
            | Block::RedstoneComparator(_)
            | Block::RedstoneTorch(_)
            | Block::RedstoneBlock
            | Block::RedstoneWallTorch(_, _)
            | Block::PressurePlate(_)
            | Block::TripwireHook(_)
            | Block::StoneButton(_)
            | Block::Target
            | Block::Lever(_) => true,
            Block::RedstoneRepeater(repeater) => {
                repeater.facing == side || repeater.facing == side.opposite()
            }
            Block::Observer(facing) => facing == side.block_facing(),
            _ => false,
        }
    }

    fn can_connect_diagonal_to(block: Block) -> bool {
        match block {
            Block::RedstoneWire(_) => true,
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
        self.north == RedstoneWireSide::None &&
        self.south == RedstoneWireSide::None &&
        self.east == RedstoneWireSide::None &&
        self.west == RedstoneWireSide::None
    }

    fn is_cross(self) -> bool {
        self.north == RedstoneWireSide::Side &&
        self.south == RedstoneWireSide::Side &&
        self.east == RedstoneWireSide::Side &&
        self.west == RedstoneWireSide::Side
    }

    fn max_wire_power(wire_power: u8, world: &dyn World, pos: BlockPos) -> u8 {
        let block = world.get_block(pos);
        if let Block::RedstoneWire(wire) = block {
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
