mod redstone_wire;

pub use redstone_wire::{RedstoneWire, RedstoneWireSide};

use crate::blocks::{Block, BlockDirection, BlockFace, BlockPos};
use crate::plot::{Plot, TickPriority};
use std::cmp;

impl Block {
    fn get_weak_power(self, plot: &Plot, pos: &BlockPos, side: &BlockFace) -> u8 {
        match self {
            Block::RedstoneTorch(true) => 15,
            Block::RedstoneWallTorch(true, _) => 15,
            Block::RedstoneBlock => 15,
            Block::Lever(lever) if lever.powered => 15,
            _ => 0,
        }
    }

    fn get_strong_power(
        self,
        plot: &Plot,
        pos: &BlockPos,
        side: BlockFace,
        dust_power: bool,
    ) -> u8 {
        match self {
            Block::RedstoneTorch(true) if side == BlockFace::Bottom => 15,
            Block::RedstoneWallTorch(true, _) if side == BlockFace::Bottom => 15,
            Block::Lever(lever) => {
                match side {
                    BlockFace::Top if lever.face == LeverFace::Floor => {
                        if lever.powered { 15 } else { 0 }
                    }
                    BlockFace::Bottom if lever.face == LeverFace::Ceiling => {
                        if lever.powered { 15 } else { 0 }
                    }
                    _ if lever.facing == side.to_direction() => {
                        if lever.powered { 15 } else { 0 }
                    }
                    _ => 0
                }
            }
            Block::RedstoneWire(wire) if dust_power => {
                let wire_pos = pos.offset(side);
                match side {
                    BlockFace::Top => wire.power,
                    BlockFace::Bottom => 0,
                    _ => {
                        let direction = side.to_direction();
                        let right_side =
                            RedstoneWire::get_side(plot, &wire_pos, direction.rotate()).is_none();
                        let left_side =
                            RedstoneWire::get_side(plot, &wire_pos, direction.rotate_ccw())
                                .is_none();
                        if right_side && left_side {
                            wire.power
                        } else {
                            0
                        }
                    }
                }
            }
            _ => 0,
        }
    }

    fn get_max_strong_power(self, plot: &Plot, pos: &BlockPos, dust_power: bool) -> u8 {
        let mut max_power = 0;
        for side in &BlockFace::values() {
            let block = plot.get_block(&pos.offset(*side));
            max_power = max_power.max(block.get_strong_power(plot, pos, *side, dust_power));
        }
        max_power
    }

    fn get_redstone_power(self, plot: &Plot, pos: &BlockPos, facing: BlockFace) -> u8 {
        if self.is_solid() {
            self.get_max_strong_power(plot, pos, true)
        } else {
            self.get_weak_power(plot, pos, &facing)
        }
    }

    fn get_redstone_power_no_dust(self, plot: &Plot, pos: &BlockPos, facing: BlockFace) -> u8 {
        if self.is_solid() {
            self.get_max_strong_power(plot, pos, false)
        } else {
            self.get_weak_power(plot, pos, &facing)
        }
    }

    pub fn torch_should_be_off(plot: &Plot, pos: &BlockPos) -> bool {
        let bottom_pos = pos.offset(BlockFace::Bottom);
        let bottom_block = plot.get_block(&bottom_pos);
        bottom_block.get_redstone_power(plot, &bottom_pos, BlockFace::Top) > 0
    }

    pub fn wall_torch_should_be_off(
        plot: &Plot,
        pos: &BlockPos,
        direction: BlockDirection,
    ) -> bool {
        let wall_pos = pos.offset(direction.opposite().block_face());
        let wall_block = plot.get_block(&wall_pos);
        wall_block.get_redstone_power(plot, &wall_pos, direction.opposite().block_face()) > 0
    }
}

fn diode_get_input_strength(plot: &Plot, pos: &BlockPos, facing: BlockDirection) -> u8 {
    let neighbor_pos = &pos.offset(facing.block_face());
    let neighbor = plot.get_block(neighbor_pos);
    let mut power = neighbor.get_redstone_power(plot, pos, facing.block_face());
    if power == 0 {
        if let Block::RedstoneWire(wire) = neighbor {
            power = wire.power;
        }
    }
    power
}

fn get_power_on_side(plot: &Plot, pos: &BlockPos, side: BlockDirection) -> u8 {
    let side_pos = &pos.offset(side.block_face());
    let side_block = plot.get_block(side_pos);
    if let Block::RedstoneWire(wire) = side_block {
        return wire.power;
    }
    side_block.get_strong_power(plot, side_pos, side.opposite().block_face(), false)

}

fn max_power_on_sides(facing: BlockDirection, plot: &Plot, pos: &BlockPos) -> u8 {
    let right_side = get_power_on_side(plot, pos, facing.rotate());
    let left_side = get_power_on_side(plot, pos, facing.rotate_ccw());
    cmp::max(right_side, left_side)
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

    pub fn get_state_for_placement(plot: &Plot, pos: &BlockPos, facing: BlockDirection) -> RedstoneRepeater {
        RedstoneRepeater {
            delay: 1,
            facing,
            locked: RedstoneRepeater::should_be_locked(facing, plot, pos),
            powered: false,
        }
    }

    fn should_be_locked(facing: BlockDirection, plot: &Plot, pos: &BlockPos) -> bool {
        max_power_on_sides(facing, plot, pos) > 0
    }

    pub fn should_be_powered(self, plot: &Plot, pos: &BlockPos) -> bool {
        diode_get_input_strength(plot, pos, self.facing) > 0
    }

    pub fn on_neighbor_updated(mut self, plot: &mut Plot, pos: &BlockPos) {
        if !self.locked && RedstoneRepeater::should_be_locked(self.facing, plot, pos) {
            self.locked = true;
            plot.set_block(pos, Block::RedstoneRepeater(self));
        }

    }

    pub fn tick(mut self, plot: &mut Plot, pos: &BlockPos) {
        if !self.locked {
            let should_be_powered = self.should_be_powered(plot, pos);
            if self.powered && should_be_powered {
                self.powered = false;
                plot.set_block(pos, Block::RedstoneRepeater(self));
            } else if !self.powered {
                self.powered = true;
                plot.set_block(pos, Block::RedstoneRepeater(self));
                if !should_be_powered {
                    plot.schedule_tick(pos, self.delay as u32, TickPriority::Higher);
                }
            }
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum LeverFace {
    Floor,
    Wall,
    Ceiling
}

impl LeverFace {
    pub(super) fn from_id(id: u32) -> LeverFace {
        match id {
            0 => LeverFace::Floor,
            1 => LeverFace::Wall,
            2 => LeverFace::Ceiling,
            _ => panic!("Invalid LeverFace"),
        }
    }

    pub(super) fn get_id(self) -> u32 {
        match self {
            LeverFace::Floor => 0,
            LeverFace::Wall => 1,
            LeverFace::Ceiling => 2,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Lever {
    pub face: LeverFace,
    pub facing: BlockDirection,
    pub powered: bool,
}

impl Lever {
    pub(super) fn new(
        face: LeverFace,
        facing: BlockDirection,
        powered: bool,
    ) -> Lever {
        Lever {
            face,
            facing,
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
