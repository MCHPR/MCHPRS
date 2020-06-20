mod redstone_wire;

pub use redstone_wire::{RedstoneWire, RedstoneWireSide};

use crate::blocks::{Block, BlockDirection, BlockEntity, BlockFace, BlockPos};
use crate::plot::{Plot, TickPriority, World};
use std::cmp;

impl Block {
    fn get_weak_power(
        self,
        plot: &dyn World,
        pos: BlockPos,
        side: BlockFace,
        dust_power: bool,
    ) -> u8 {
        match self {
            Block::RedstoneTorch(true) => 15,
            Block::RedstoneWallTorch(true, _) => 15,
            Block::RedstoneBlock => 15,
            Block::Lever(lever) if lever.powered => 15,
            Block::RedstoneRepeater(repeater)
                if repeater.facing.block_face() == side && repeater.powered =>
            {
                15
            }
            Block::RedstoneComparator(comparator) if comparator.facing.block_face() == side => {
                if let Some(BlockEntity::Comparator { output_strength }) =
                    plot.get_block_entity(pos)
                {
                    output_strength
                } else {
                    0
                }
            }
            Block::RedstoneWire(wire) if dust_power => match side {
                BlockFace::Top => wire.power,
                BlockFace::Bottom => 0,
                _ => {
                    let direction = side.to_direction();
                    let right_side =
                        RedstoneWire::get_side(plot, pos, direction.rotate()).is_none();
                    let left_side =
                        RedstoneWire::get_side(plot, pos, direction.rotate_ccw()).is_none();
                    if right_side && left_side {
                        wire.power
                    } else {
                        0
                    }
                }
            },
            _ => 0,
        }
    }

    fn get_strong_power(
        self,
        plot: &dyn World,
        pos: BlockPos,
        side: BlockFace,
        dust_power: bool,
    ) -> u8 {
        match self {
            Block::RedstoneTorch(true) if side == BlockFace::Bottom => 15,
            Block::RedstoneWallTorch(true, _) if side == BlockFace::Bottom => 15,
            Block::Lever(lever) => match side {
                BlockFace::Top if lever.face == LeverFace::Floor => {
                    if lever.powered {
                        15
                    } else {
                        0
                    }
                }
                BlockFace::Bottom if lever.face == LeverFace::Ceiling => {
                    if lever.powered {
                        15
                    } else {
                        0
                    }
                }
                _ if lever.facing == side.to_direction() => {
                    if lever.powered {
                        15
                    } else {
                        0
                    }
                }
                _ => 0,
            },
            Block::RedstoneWire(_) => self.get_weak_power(plot, pos, side, dust_power),
            Block::RedstoneRepeater(_) => self.get_weak_power(plot, pos, side, dust_power),
            Block::RedstoneComparator(_) => self.get_weak_power(plot, pos, side, dust_power),
            _ => 0,
        }
    }

    fn get_max_strong_power(self, plot: &dyn World, pos: BlockPos, dust_power: bool) -> u8 {
        let mut max_power = 0;
        for side in &BlockFace::values() {
            let block = plot.get_block(pos.offset(*side));
            max_power =
                max_power.max(block.get_strong_power(plot, pos.offset(*side), *side, dust_power));
        }
        max_power
    }

    pub fn get_redstone_power(self, plot: &dyn World, pos: BlockPos, facing: BlockFace) -> u8 {
        if self.is_solid() {
            self.get_max_strong_power(plot, pos, true)
        } else {
            self.get_weak_power(plot, pos, facing, true)
        }
    }

    fn get_redstone_power_no_dust(self, plot: &dyn World, pos: BlockPos, facing: BlockFace) -> u8 {
        if self.is_solid() {
            self.get_max_strong_power(plot, pos, false)
        } else {
            self.get_weak_power(plot, pos, facing, false)
        }
    }

    pub fn torch_should_be_off(plot: &dyn World, pos: BlockPos) -> bool {
        let bottom_pos = pos.offset(BlockFace::Bottom);
        let bottom_block = plot.get_block(bottom_pos);
        bottom_block.get_redstone_power(plot, bottom_pos, BlockFace::Top) > 0
    }

    pub fn wall_torch_should_be_off(
        plot: &dyn World,
        pos: BlockPos,
        direction: BlockDirection,
    ) -> bool {
        let wall_pos = pos.offset(direction.opposite().block_face());
        let wall_block = plot.get_block(wall_pos);
        wall_block.get_redstone_power(plot, wall_pos, direction.opposite().block_face()) > 0
    }

    pub fn redstone_lamp_should_be_lit(plot: &dyn World, pos: BlockPos) -> bool {
        for face in &BlockFace::values() {
            let neighbor_pos = pos.offset(*face);
            if plot
                .get_block(neighbor_pos)
                .get_redstone_power(plot, neighbor_pos, *face)
                > 0
            {
                return true;
            }
        }
        false
    }
}

fn diode_get_input_strength(plot: &dyn World, pos: BlockPos, facing: BlockDirection) -> u8 {
    let input_pos = pos.offset(facing.block_face());
    let input_block = plot.get_block(input_pos);
    let mut power = input_block.get_redstone_power(plot, input_pos, facing.block_face());
    if power == 0 {
        if let Block::RedstoneWire(wire) = input_block {
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

impl Default for RedstoneRepeater {
    fn default() -> Self {
        RedstoneRepeater {
            delay: 1,
            facing: Default::default(),
            locked: false,
            powered: false,
        }
    }
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

    pub fn get_state_for_placement(
        plot: &Plot,
        pos: BlockPos,
        facing: BlockDirection,
    ) -> RedstoneRepeater {
        RedstoneRepeater {
            delay: 1,
            facing,
            locked: RedstoneRepeater::should_be_locked(facing, plot, pos),
            powered: false,
        }
    }

    fn should_be_locked(facing: BlockDirection, plot: &dyn World, pos: BlockPos) -> bool {
        let right_side = RedstoneRepeater::get_power_on_side(plot, pos, facing.rotate());
        let left_side = RedstoneRepeater::get_power_on_side(plot, pos, facing.rotate_ccw());
        cmp::max(right_side, left_side) > 0
    }

    fn get_power_on_side(plot: &dyn World, pos: BlockPos, side: BlockDirection) -> u8 {
        let side_pos = pos.offset(side.block_face());
        let side_block = plot.get_block(side_pos);
        if side_block.is_diode() {
            side_block.get_weak_power(plot, side_pos, side.block_face(), false)
        } else {
            0
        }
    }

    fn on_state_change(self, plot: &mut dyn World, pos: BlockPos) {
        let front_pos = pos.offset(self.facing.opposite().block_face());
        let front_block = plot.get_block(front_pos);
        front_block.update(plot, front_pos);
        for direction in &BlockFace::values() {
            let neighbor_pos = front_pos.offset(*direction);
            let block = plot.get_block(neighbor_pos);
            block.update(plot, neighbor_pos);
        }
    }

    pub fn schedule_tick(self, plot: &mut dyn World, pos: BlockPos, should_be_powered: bool) {
        let front_block = plot.get_block(pos.offset(self.facing.opposite().block_face()));
        let priority = if front_block.is_diode() {
            TickPriority::Highest
        } else if !should_be_powered {
            TickPriority::Higher
        } else {
            TickPriority::High
        };
        plot.schedule_tick(pos, self.delay as u32, priority);
    }

    pub fn should_be_powered(self, plot: &dyn World, pos: BlockPos) -> bool {
        diode_get_input_strength(plot, pos, self.facing) > 0
    }

    pub fn on_neighbor_updated(mut self, plot: &mut dyn World, pos: BlockPos) {
        let should_be_locked = RedstoneRepeater::should_be_locked(self.facing, plot, pos);
        if !self.locked && should_be_locked {
            self.locked = true;
            plot.set_block(pos, Block::RedstoneRepeater(self));
        } else if self.locked && !should_be_locked {
            self.locked = false;
            plot.set_block(pos, Block::RedstoneRepeater(self));
        }

        if !self.locked && !plot.pending_tick_at(pos) {
            let should_be_powered = self.should_be_powered(plot, pos);
            if should_be_powered != self.powered {
                self.schedule_tick(plot, pos, should_be_powered);
            }
        }
    }

    pub fn tick(mut self, plot: &mut dyn World, pos: BlockPos) {
        if self.locked {
            return;
        }

        let should_be_powered = self.should_be_powered(plot, pos);
        if self.powered && !should_be_powered {
            self.powered = false;
            plot.set_block(pos, Block::RedstoneRepeater(self));
            self.on_state_change(plot, pos);
        } else if !self.powered {
            self.powered = true;
            plot.set_block(pos, Block::RedstoneRepeater(self));
            self.on_state_change(plot, pos);
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

    pub(super) fn toggle(self) -> ComparatorMode {
        match self {
            ComparatorMode::Subtract => ComparatorMode::Compare,
            ComparatorMode::Compare => ComparatorMode::Subtract,
        }
    }

    pub(super) fn from_str(name: &str) -> ComparatorMode {
        match name {
            "subtract" => ComparatorMode::Subtract,
            "compare" => ComparatorMode::Compare,
            _ => ComparatorMode::Compare,
        }
    }
}

impl Default for ComparatorMode {
    fn default() -> Self {
        ComparatorMode::Compare
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Default)]
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

    fn get_power_on_side(plot: &dyn World, pos: BlockPos, side: BlockDirection) -> u8 {
        let side_pos = pos.offset(side.block_face());
        let side_block = plot.get_block(side_pos);
        if side_block.is_diode() {
            side_block.get_weak_power(plot, side_pos, side.block_face(), false)
        } else if let Block::RedstoneWire(wire) = side_block {
            wire.power
        } else {
            0
        }
    }

    fn max_power_on_sides(self, plot: &dyn World, pos: BlockPos) -> u8 {
        let right_side = RedstoneComparator::get_power_on_side(plot, pos, self.facing.rotate());
        let left_side = RedstoneComparator::get_power_on_side(plot, pos, self.facing.rotate_ccw());
        cmp::max(right_side, left_side)
    }

    fn calculate_input_strength(self, plot: &dyn World, pos: BlockPos) -> u8 {
        let base_input_strength = diode_get_input_strength(plot, pos, self.facing);
        let input_pos = pos.offset(self.facing.block_face());
        let input_block = plot.get_block(input_pos);
        if input_block.has_comparator_override() {
            input_block.get_comparator_override(plot, input_pos)
        } else if base_input_strength < 15 && input_block.is_cube() {
            let far_input_pos = input_pos.offset(self.facing.block_face());
            let far_input_block = plot.get_block(far_input_pos);
            if far_input_block.has_comparator_override() {
                far_input_block.get_comparator_override(plot, far_input_pos)
            } else {
                base_input_strength
            }
        } else {
            base_input_strength
        }
    }

    fn get_power_on_sides(self, plot: &dyn World, pos: BlockPos) -> u8 {
        cmp::max(
            RedstoneComparator::get_power_on_side(plot, pos, self.facing.rotate()),
            RedstoneComparator::get_power_on_side(plot, pos, self.facing.rotate_ccw()),
        )
    }

    pub fn should_be_powered(self, plot: &dyn World, pos: BlockPos) -> bool {
        let input_strength = self.calculate_input_strength(plot, pos);
        if input_strength == 0 {
            false
        } else {
            let power_on_sides = self.get_power_on_sides(plot, pos);
            if input_strength > power_on_sides {
                true
            } else {
                power_on_sides == input_strength && self.mode == ComparatorMode::Compare
            }
        }
    }

    fn calculate_output_strength(self, plot: &mut dyn World, pos: BlockPos) -> u8 {
        let input_strength = Self::calculate_input_strength(self, plot, pos);
        if self.mode == ComparatorMode::Subtract {
            input_strength.saturating_sub(self.max_power_on_sides(plot, pos))
        } else if input_strength >= self.max_power_on_sides(plot, pos) {
            input_strength
        } else {
            0
        }
    }

    // This is exactly the same as it is in the RedstoneRepeater struct.
    // Sometime in the future, this needs to be reused. LLVM might optimize
    // it way, but te human brane wil not!
    fn on_state_change(self, plot: &mut dyn World, pos: BlockPos) {
        let front_pos = pos.offset(self.facing.opposite().block_face());
        let front_block = plot.get_block(front_pos);
        front_block.update(plot, front_pos);
        for direction in &BlockFace::values() {
            let neighbor_pos = front_pos.offset(*direction);
            let block = plot.get_block(neighbor_pos);
            block.update(plot, neighbor_pos);
        }
    }

    pub fn update(self, plot: &mut dyn World, pos: BlockPos) {
        if plot.pending_tick_at(pos) {
            return;
        }
        let output_strength = self.calculate_output_strength(plot, pos);
        let old_strength =
            if let Some(BlockEntity::Comparator { output_strength }) = plot.get_block_entity(pos) {
                output_strength
            } else {
                0
            };
        if output_strength != old_strength || self.powered != self.should_be_powered(plot, pos) {
            let front_block = plot.get_block(pos.offset(self.facing.opposite().block_face()));
            let priority = if front_block.is_diode() {
                TickPriority::High
            } else {
                TickPriority::Normal
            };
            plot.schedule_tick(pos, 1, priority);
        }
    }

    pub fn tick(mut self, plot: &mut dyn World, pos: BlockPos) {
        let new_strength = self.calculate_output_strength(plot, pos);
        let old_strength = if let Some(BlockEntity::Comparator {
            output_strength: old_output_strength,
        }) = plot.get_block_entity(pos)
        {
            old_output_strength
        } else {
            0
        };
        if new_strength != old_strength || self.mode == ComparatorMode::Compare {
            plot.set_block_entity(
                pos,
                BlockEntity::Comparator {
                    output_strength: new_strength,
                },
            );
            let should_be_powered = self.should_be_powered(plot, pos);
            let powered = self.powered;
            if powered && !should_be_powered {
                self.powered = false;
                plot.set_block(pos, Block::RedstoneComparator(self));
            } else if !powered && should_be_powered {
                self.powered = true;
                plot.set_block(pos, Block::RedstoneComparator(self));
            }
            self.on_state_change(plot, pos);
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum LeverFace {
    Floor,
    Wall,
    Ceiling,
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

    pub(super) fn from_str(name: &str) -> LeverFace {
        match name {
            "floor" => LeverFace::Floor,
            "ceiling" => LeverFace::Ceiling,
            _ => LeverFace::Wall,
        }
    }
}

impl Default for LeverFace {
    fn default() -> Self {
        LeverFace::Wall
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub struct Lever {
    pub face: LeverFace,
    pub facing: BlockDirection,
    pub powered: bool,
}

impl Lever {
    pub(super) fn new(face: LeverFace, facing: BlockDirection, powered: bool) -> Lever {
        Lever {
            face,
            facing,
            powered,
        }
    }
}
