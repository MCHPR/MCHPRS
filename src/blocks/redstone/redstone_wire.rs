use crate::blocks::{Block, BlockDirection, BlockFace, BlockPos};
use crate::plot::Plot;

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

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct RedstoneWire {
    pub north: RedstoneWireSide,
    pub south: RedstoneWireSide,
    pub east: RedstoneWireSide,
    pub west: RedstoneWireSide,
    pub power: u8,
}

impl RedstoneWire {
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

    pub fn get_state_for_placement(plot: &Plot, pos: BlockPos) -> RedstoneWire {
        RedstoneWire {
            power: RedstoneWire::calculate_power(plot, pos),
            north: RedstoneWire::get_side(plot, pos, BlockDirection::North),
            south: RedstoneWire::get_side(plot, pos, BlockDirection::South),
            east: RedstoneWire::get_side(plot, pos, BlockDirection::East),
            west: RedstoneWire::get_side(plot, pos, BlockDirection::West),
        }
    }

    pub fn on_neighbor_changed(
        mut self,
        plot: &Plot,
        pos: BlockPos,
        side: &BlockFace,
    ) -> RedstoneWire {
        match side {
            BlockFace::Top => {}
            BlockFace::Bottom => {
                self.north = RedstoneWire::get_side(plot, pos, BlockDirection::North);
                self.south = RedstoneWire::get_side(plot, pos, BlockDirection::South);
                self.east = RedstoneWire::get_side(plot, pos, BlockDirection::East);
                self.west = RedstoneWire::get_side(plot, pos, BlockDirection::West);
            }
            BlockFace::North => {
                self.south = RedstoneWire::get_side(plot, pos, BlockDirection::South)
            }
            BlockFace::South => {
                self.north = RedstoneWire::get_side(plot, pos, BlockDirection::North)
            }

            BlockFace::East => self.west = RedstoneWire::get_side(plot, pos, BlockDirection::West),
            BlockFace::West => self.east = RedstoneWire::get_side(plot, pos, BlockDirection::East),
        }
        self
    }

    pub fn on_neighbor_updated(mut self, plot: &mut Plot, pos: BlockPos) {
        let new_power = RedstoneWire::calculate_power(plot, pos);

        if self.power != new_power {
            self.power = new_power;
            plot.set_block(pos, Block::RedstoneWire(self));

            Block::update_wire_neighbors(plot, pos);
        }
    }

    fn can_connect_to(block: &Block, side: BlockDirection) -> bool {
        match block {
            Block::RedstoneWire(_)
            | Block::RedstoneComparator(_)
            | Block::RedstoneTorch(_)
            | Block::RedstoneBlock
            | Block::RedstoneWallTorch(_, _)
            | Block::Lever(_) => true,
            Block::RedstoneRepeater(repeater) => {
                repeater.facing == side || repeater.facing == side.opposite()
            }
            _ => false,
        }
    }

    fn can_connect_diagonal_to(block: &Block) -> bool {
        match block {
            Block::RedstoneWire(_) => true,
            _ => false,
        }
    }

    pub fn get_side(plot: &Plot, pos: BlockPos, side: BlockDirection) -> RedstoneWireSide {
        let neighbor_pos = pos.offset(side.block_face());
        let neighbor = plot.get_block(neighbor_pos);

        if RedstoneWire::can_connect_to(&neighbor, side) {
            return RedstoneWireSide::Side;
        }

        let up_pos = pos.offset(BlockFace::Top);
        let up = plot.get_block(up_pos);

        if !up.is_solid()
            && RedstoneWire::can_connect_diagonal_to(
                &plot.get_block(neighbor_pos.offset(BlockFace::Top)),
            )
        {
            RedstoneWireSide::Up
        } else if !neighbor.is_solid()
            && RedstoneWire::can_connect_diagonal_to(
                &plot.get_block(neighbor_pos.offset(BlockFace::Bottom)),
            )
        {
            RedstoneWireSide::Side
        } else {
            RedstoneWireSide::None
        }
    }

    fn max_wire_power(wire_power: u8, plot: &Plot, pos: BlockPos) -> u8 {
        let block = plot.get_block(pos);
        if let Block::RedstoneWire(wire) = block {
            wire_power.max(wire.power)
        } else {
            wire_power
        }
    }

    fn calculate_power(plot: &Plot, pos: BlockPos) -> u8 {
        let mut block_power = 0;
        let mut wire_power = 0;

        let up_pos = pos.offset(BlockFace::Top);
        let up_block = plot.get_block(up_pos);

        for side in &BlockFace::values() {
            let neighbor_pos = pos.offset(*side);
            wire_power = RedstoneWire::max_wire_power(wire_power, plot, neighbor_pos);
            let neighbor = plot.get_block(neighbor_pos);
            block_power =
                block_power.max(neighbor.get_redstone_power_no_dust(plot, neighbor_pos, *side));
            if side.is_horizontal() {
                if !up_block.is_solid() && !neighbor.is_transparent() {
                    wire_power = RedstoneWire::max_wire_power(
                        wire_power,
                        plot,
                        neighbor_pos.offset(BlockFace::Top),
                    );
                }

                if !neighbor.is_solid() {
                    wire_power = RedstoneWire::max_wire_power(
                        wire_power,
                        plot,
                        neighbor_pos.offset(BlockFace::Bottom),
                    );
                }
            }
        }

        block_power.max(wire_power.saturating_sub(1))
    }
}
