use crate::blocks::{Block, BlockDirection, BlockFace, BlockPos};
use crate::plot::Plot;

impl Block {
    fn get_weak_power(self, plot: &Plot, pos: &BlockPos, side: &BlockFace) -> u8 {
        match self {
            Block::RedstoneTorch(true) => 15,
            Block::RedstoneWallTorch(true, _) => 15,
            Block::RedstoneBlock => 15,
            _ => 0,
        }
    }

    fn get_strong_power(self, plot: &Plot, pos: &BlockPos) -> u8 {
        match self {
            _ => 0,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum RedstoneWireSide {
    Up,
    Side,
    None,
}

impl RedstoneWireSide {
    pub(super) fn from_id(id: u32) -> RedstoneWireSide {
        match id {
            0 => RedstoneWireSide::Up,
            1 => RedstoneWireSide::Side,
            2 => RedstoneWireSide::None,
            _ => panic!("Invalid RedstoneWireSide"),
        }
    }

    pub(super) fn get_id(self) -> u32 {
        match self {
            RedstoneWireSide::Up => 0,
            RedstoneWireSide::Side => 1,
            RedstoneWireSide::None => 2,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct RedstoneWire {
    pub(super) north: RedstoneWireSide,
    pub(super) south: RedstoneWireSide,
    pub(super) east: RedstoneWireSide,
    pub(super) west: RedstoneWireSide,
    pub(super) power: u8,
}

impl RedstoneWire {
    pub(super) fn new(
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

    pub fn get_state_for_placement(plot: &Plot, pos: &BlockPos) -> RedstoneWire {
        RedstoneWire {
            power: RedstoneWire::calculate_power(plot, pos),
            north: RedstoneWire::get_side(plot, &pos, BlockDirection::North),
            south: RedstoneWire::get_side(plot, &pos, BlockDirection::South),
            east: RedstoneWire::get_side(plot, &pos, BlockDirection::East),
            west: RedstoneWire::get_side(plot, &pos, BlockDirection::West),
        }
    }

    pub fn on_neighbor_changed(
        mut self,
        plot: &Plot,
        pos: &BlockPos,
        side: &BlockFace,
    ) -> RedstoneWire {
        match side {
            BlockFace::Top => {}
            BlockFace::Bottom => {
                self.north = RedstoneWire::get_side(plot, &pos, BlockDirection::North);
                self.south = RedstoneWire::get_side(plot, &pos, BlockDirection::South);
                self.east = RedstoneWire::get_side(plot, &pos, BlockDirection::East);
                self.west = RedstoneWire::get_side(plot, &pos, BlockDirection::West);
            }
            BlockFace::North => {
                self.south = RedstoneWire::get_side(plot, &pos, BlockDirection::South)
            }
            BlockFace::South => {
                self.north = RedstoneWire::get_side(plot, &pos, BlockDirection::North)
            }

            BlockFace::East => self.west = RedstoneWire::get_side(plot, &pos, BlockDirection::West),
            BlockFace::West => self.east = RedstoneWire::get_side(plot, &pos, BlockDirection::East),
        }
        self
    }

    pub fn on_neighbor_updated(mut self, plot: &mut Plot, pos: &BlockPos) {
        let new_power = RedstoneWire::calculate_power(plot, pos);

        if self.power != new_power {
            self.power = new_power;
            plot.set_block(pos, Block::RedstoneWire(self));
            Block::update_surrounding_blocks(plot, pos);
        }
    }

    fn can_connect_to(block: &Block, side: BlockDirection) -> bool {
        match block {
            Block::RedstoneWire(_)
            | Block::RedstoneComparator(_)
            | Block::RedstoneTorch(_)
            | Block::RedstoneWallTorch(_, _) => true,
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

    fn get_side(plot: &Plot, pos: &BlockPos, side: BlockDirection) -> RedstoneWireSide {
        let neighbor_pos = &pos.offset(side.block_face());
        let neighbor = plot.get_block(neighbor_pos);

        if RedstoneWire::can_connect_to(&neighbor, side) {
            return RedstoneWireSide::Side;
        }

        let up_pos = pos.offset(BlockFace::Top);
        let up = plot.get_block(&up_pos);

        if !up.is_solid()
            && RedstoneWire::can_connect_diagonal_to(
                &plot.get_block(&neighbor_pos.offset(BlockFace::Top)),
            )
        {
            RedstoneWireSide::Up
        } else if !neighbor.is_solid()
            && RedstoneWire::can_connect_diagonal_to(
                &plot.get_block(&neighbor_pos.offset(BlockFace::Bottom)),
            )
        {
            RedstoneWireSide::Side
        } else {
            RedstoneWireSide::None
        }
    }

    fn max_wire_power(wire_power: u8, plot: &Plot, pos: &BlockPos) -> u8 {
        let block = plot.get_block(pos);
        if let Block::RedstoneWire(wire) = block {
            wire_power.max(wire.power)
        } else {
            wire_power
        }
    }

    fn calculate_power(plot: &Plot, pos: &BlockPos) -> u8 {
        let mut block_power = 0;
        let mut wire_power = 0;

        let up_pos = &pos.offset(BlockFace::Top);
        let up_block = plot.get_block(up_pos);

        for side in &BlockFace::values() {
            let neighbor_pos = &pos.offset(*side);
            wire_power = RedstoneWire::max_wire_power(wire_power, plot, neighbor_pos);
            let neighbor = plot.get_block(neighbor_pos);
            if neighbor.is_solid() || neighbor.is_transparent() {
                block_power = block_power.max(neighbor.get_strong_power(plot, neighbor_pos));
            } else {
                block_power = block_power.max(neighbor.get_weak_power(plot, neighbor_pos, side));
            }
            if side.is_horizontal() {
                if !up_block.is_solid() && !neighbor.is_transparent() {
                    wire_power = RedstoneWire::max_wire_power(
                        wire_power,
                        plot,
                        &neighbor_pos.offset(BlockFace::Top),
                    );
                }

                if !neighbor.is_solid() {
                    wire_power = RedstoneWire::max_wire_power(
                        wire_power,
                        plot,
                        &neighbor_pos.offset(BlockFace::Bottom),
                    );
                }
            }
        }

        block_power.max(wire_power.saturating_sub(1))
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct RedstoneRepeater {
    pub(super) delay: u8,
    pub(super) facing: BlockDirection,
    pub(super) locked: bool,
    pub(super) powered: bool,
}

impl RedstoneRepeater {
    pub(super) fn new(
        delay: u8,
        facing: BlockDirection,
        locked: bool,
        powered: bool,
    ) -> RedstoneRepeater {
        RedstoneRepeater {
            delay,
            facing,
            locked,
            powered,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ComparatorMode {
    Compare,
    Subtract,
}

impl ComparatorMode {
    pub(super) fn from_id(id: u32) -> ComparatorMode {
        match id {
            0 => ComparatorMode::Compare,
            1 => ComparatorMode::Subtract,
            _ => panic!("Invalid ComparatorMode"),
        }
    }

    pub(super) fn get_id(self) -> u32 {
        match self {
            ComparatorMode::Compare => 0,
            ComparatorMode::Subtract => 1,
        }
    }

    pub(super) fn flip(self) -> ComparatorMode {
        match self {
            ComparatorMode::Subtract => ComparatorMode::Compare,
            ComparatorMode::Compare => ComparatorMode::Subtract,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct RedstoneComparator {
    pub(super) facing: BlockDirection,
    pub(super) mode: ComparatorMode,
    pub(super) powered: bool,
}

impl RedstoneComparator {
    pub(super) fn new(
        facing: BlockDirection,
        mode: ComparatorMode,
        powered: bool,
    ) -> RedstoneComparator {
        RedstoneComparator {
            facing,
            mode,
            powered,
        }
    }
}
