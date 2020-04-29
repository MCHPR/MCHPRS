mod redstone_wire;

pub use redstone_wire::{RedstoneWire, RedstoneWireSide};

use crate::blocks::{Block, BlockDirection, BlockFace, BlockPos};
use crate::plot::{Plot, TickPriority};

impl Block {
    fn get_weak_power(self, plot: &Plot, pos: &BlockPos, side: &BlockFace) -> u8 {
        match self {
            Block::RedstoneTorch(true) => 15,
            Block::RedstoneWallTorch(true, _) => 15,
            Block::RedstoneBlock => 15,
            _ => 0,
        }
    }

    fn get_strong_power(self, plot: &Plot, pos: &BlockPos, side: BlockFace) -> u8 {
        match self {
            Block::RedstoneTorch(true) if side == BlockFace::Bottom => 15,
            Block::RedstoneWallTorch(true, _) if side == BlockFace::Bottom => 15,
            Block::RedstoneWire(wire) => wire.power,
            _ => 0,
        }
    }

    fn get_max_strong_power(self, plot: &Plot, pos: &BlockPos) -> u8 {
        let mut max_power = 0;
        for side in &BlockFace::values() {
            let block = plot.get_block(&pos.offset(*side));
            max_power = max_power.max(block.get_strong_power(plot, pos, *side));
        }
        max_power
    }

    fn get_redstone_power(&self, plot: &Plot, pos: &BlockPos, facing: BlockFace) -> u8 {
        if self.is_solid() {
            self.get_max_strong_power(plot, pos)
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

    pub fn should_be_powered(&self, plot: &Plot, pos: &BlockPos) -> bool {
        diode_get_input_strength(plot, pos, self.facing) > 0
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
