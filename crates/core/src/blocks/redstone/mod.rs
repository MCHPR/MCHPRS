mod redstone_wire;

use crate::blocks::{
    Block, BlockDirection, BlockEntity, BlockFace, BlockPos, BlockProperty, BlockTransform,
};
use crate::world::World;
use mchprs_world::TickPriority;
pub use redstone_wire::{RedstoneWire, RedstoneWireSide};
use std::cmp;
use std::str::FromStr;

impl Block {
    fn get_weak_power(
        self,
        world: &impl World,
        pos: BlockPos,
        side: BlockFace,
        dust_power: bool,
    ) -> u8 {
        match self {
            Block::RedstoneTorch { lit: true } => 15,
            Block::RedstoneWallTorch { lit: true, facing } if facing.block_face() != side => 15,
            Block::RedstoneBlock {} => 15,
            Block::StonePressurePlate { powered: true } => 15,
            Block::Lever { lever } if lever.powered => 15,
            Block::StoneButton { button } if button.powered => 15,
            Block::RedstoneRepeater { repeater }
                if repeater.facing.block_face() == side && repeater.powered =>
            {
                15
            }
            Block::RedstoneComparator { comparator } if comparator.facing.block_face() == side => {
                if let Some(BlockEntity::Comparator { output_strength }) =
                    world.get_block_entity(pos)
                {
                    *output_strength
                } else {
                    0
                }
            }
            Block::RedstoneWire { wire } if dust_power => match side {
                BlockFace::Top => wire.power,
                BlockFace::Bottom => 0,
                _ => {
                    let direction = side.to_direction();
                    if wire
                        .get_regulated_sides(world, pos)
                        .get_current_side(direction.opposite())
                        .is_none()
                    {
                        0
                    } else {
                        wire.power
                    }
                }
            },
            _ => 0,
        }
    }

    fn get_strong_power(
        self,
        world: &impl World,
        pos: BlockPos,
        side: BlockFace,
        dust_power: bool,
    ) -> u8 {
        match self {
            Block::RedstoneTorch { lit: true } if side == BlockFace::Bottom => 15,
            Block::RedstoneWallTorch { lit: true, .. } if side == BlockFace::Bottom => 15,
            Block::Lever { lever } => match side {
                BlockFace::Top if lever.face == LeverFace::Floor && lever.powered => 15,
                BlockFace::Bottom if lever.face == LeverFace::Ceiling && lever.powered => 15,
                _ if lever.facing == side.to_direction() && lever.powered => 15,
                _ => 0,
            },
            Block::StoneButton { button } => match side {
                BlockFace::Top if button.face == ButtonFace::Floor && button.powered => 15,
                BlockFace::Bottom if button.face == ButtonFace::Ceiling && button.powered => 15,
                _ if button.facing == side.to_direction() && button.powered => 15,
                _ => 0,
            },
            Block::StonePressurePlate { powered: true } if side == BlockFace::Top => 15,
            Block::RedstoneWire { .. } => self.get_weak_power(world, pos, side, dust_power),
            Block::RedstoneRepeater { .. } => self.get_weak_power(world, pos, side, dust_power),
            Block::RedstoneComparator { .. } => self.get_weak_power(world, pos, side, dust_power),
            _ => 0,
        }
    }

    fn get_max_strong_power(self, world: &impl World, pos: BlockPos, dust_power: bool) -> u8 {
        let mut max_power = 0;
        for side in &BlockFace::values() {
            let block = world.get_block(pos.offset(*side));
            max_power =
                max_power.max(block.get_strong_power(world, pos.offset(*side), *side, dust_power));
        }
        max_power
    }

    pub fn get_redstone_power(self, world: &impl World, pos: BlockPos, facing: BlockFace) -> u8 {
        if self.is_solid() {
            self.get_max_strong_power(world, pos, true)
        } else {
            self.get_weak_power(world, pos, facing, true)
        }
    }

    fn get_redstone_power_no_dust(
        self,
        world: &impl World,
        pos: BlockPos,
        facing: BlockFace,
    ) -> u8 {
        if self.is_solid() {
            self.get_max_strong_power(world, pos, false)
        } else {
            self.get_weak_power(world, pos, facing, false)
        }
    }

    pub fn torch_should_be_off(world: &impl World, pos: BlockPos) -> bool {
        let bottom_pos = pos.offset(BlockFace::Bottom);
        let bottom_block = world.get_block(bottom_pos);
        bottom_block.get_redstone_power(world, bottom_pos, BlockFace::Top) > 0
    }

    pub fn wall_torch_should_be_off(
        world: &impl World,
        pos: BlockPos,
        direction: BlockDirection,
    ) -> bool {
        let wall_pos = pos.offset(direction.opposite().block_face());
        let wall_block = world.get_block(wall_pos);
        wall_block.get_redstone_power(world, wall_pos, direction.opposite().block_face()) > 0
    }

    pub fn redstone_lamp_should_be_lit(world: &impl World, pos: BlockPos) -> bool {
        for face in &BlockFace::values() {
            let neighbor_pos = pos.offset(*face);
            if world
                .get_block(neighbor_pos)
                .get_redstone_power(world, neighbor_pos, *face)
                > 0
            {
                return true;
            }
        }
        false
    }
}

fn diode_get_input_strength(world: &impl World, pos: BlockPos, facing: BlockDirection) -> u8 {
    let input_pos = pos.offset(facing.block_face());
    let input_block = world.get_block(input_pos);
    let mut power = input_block.get_redstone_power(world, input_pos, facing.block_face());
    if power == 0 {
        if let Block::RedstoneWire { wire } = input_block {
            power = wire.power;
        }
    }
    power
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, BlockProperty, BlockTransform)]
pub struct RedstoneRepeater {
    pub delay: u8,
    pub facing: BlockDirection,
    pub locked: bool,
    pub powered: bool,
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
        world: &impl World,
        pos: BlockPos,
        facing: BlockDirection,
    ) -> RedstoneRepeater {
        RedstoneRepeater {
            delay: 1,
            facing,
            locked: RedstoneRepeater::should_be_locked(facing, world, pos),
            powered: false,
        }
    }

    fn should_be_locked(facing: BlockDirection, world: &impl World, pos: BlockPos) -> bool {
        let right_side = RedstoneRepeater::get_power_on_side(world, pos, facing.rotate());
        let left_side = RedstoneRepeater::get_power_on_side(world, pos, facing.rotate_ccw());
        cmp::max(right_side, left_side) > 0
    }

    fn get_power_on_side(world: &impl World, pos: BlockPos, side: BlockDirection) -> u8 {
        let side_pos = pos.offset(side.block_face());
        let side_block = world.get_block(side_pos);
        if side_block.is_diode() {
            side_block.get_weak_power(world, side_pos, side.block_face(), false)
        } else {
            0
        }
    }

    fn on_state_change(self, world: &mut impl World, pos: BlockPos) {
        let front_pos = pos.offset(self.facing.opposite().block_face());
        let front_block = world.get_block(front_pos);
        front_block.update(world, front_pos);
        for direction in &BlockFace::values() {
            let neighbor_pos = front_pos.offset(*direction);
            let block = world.get_block(neighbor_pos);
            block.update(world, neighbor_pos);
        }
    }

    pub fn schedule_tick(self, world: &mut impl World, pos: BlockPos, should_be_powered: bool) {
        let front_block = world.get_block(pos.offset(self.facing.opposite().block_face()));
        let priority = if front_block.is_diode() {
            TickPriority::Highest
        } else if !should_be_powered {
            TickPriority::Higher
        } else {
            TickPriority::High
        };
        world.schedule_tick(pos, self.delay as u32, priority);
    }

    pub fn should_be_powered(self, world: &impl World, pos: BlockPos) -> bool {
        diode_get_input_strength(world, pos, self.facing) > 0
    }

    pub fn on_neighbor_updated(mut self, world: &mut impl World, pos: BlockPos) {
        let should_be_locked = RedstoneRepeater::should_be_locked(self.facing, world, pos);
        if !self.locked && should_be_locked {
            self.locked = true;
            world.set_block(pos, Block::RedstoneRepeater { repeater: self });
        } else if self.locked && !should_be_locked {
            self.locked = false;
            world.set_block(pos, Block::RedstoneRepeater { repeater: self });
        }

        if !self.locked && !world.pending_tick_at(pos) {
            let should_be_powered = self.should_be_powered(world, pos);
            if should_be_powered != self.powered {
                self.schedule_tick(world, pos, should_be_powered);
            }
        }
    }

    pub fn tick(mut self, world: &mut impl World, pos: BlockPos) {
        if self.locked {
            return;
        }

        let should_be_powered = self.should_be_powered(world, pos);
        if self.powered && !should_be_powered {
            self.powered = false;
            world.set_block(pos, Block::RedstoneRepeater { repeater: self });
            self.on_state_change(world, pos);
        } else if !self.powered {
            self.powered = true;
            world.set_block(pos, Block::RedstoneRepeater { repeater: self });
            self.on_state_change(world, pos);
        }
    }
}

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
pub enum ComparatorMode {
    #[default]
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
}

impl FromStr for ComparatorMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "subtract" => ComparatorMode::Subtract,
            "compare" => ComparatorMode::Compare,
            _ => return Err(()),
        })
    }
}

impl ToString for ComparatorMode {
    fn to_string(&self) -> String {
        match self {
            ComparatorMode::Subtract => "subtract".to_owned(),
            ComparatorMode::Compare => "compare".to_owned(),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, BlockProperty, BlockTransform)]
pub struct RedstoneComparator {
    pub facing: BlockDirection,
    pub mode: ComparatorMode,
    pub powered: bool,
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

    fn get_power_on_side(world: &impl World, pos: BlockPos, side: BlockDirection) -> u8 {
        let side_pos = pos.offset(side.block_face());
        let side_block = world.get_block(side_pos);
        if side_block.is_diode() {
            side_block.get_weak_power(world, side_pos, side.block_face(), false)
        } else if let Block::RedstoneWire { wire } = side_block {
            wire.power
        } else if let Block::RedstoneBlock {} = side_block {
            15
        } else {
            0
        }
    }

    fn max_power_on_sides(self, world: &impl World, pos: BlockPos) -> u8 {
        let right_side = RedstoneComparator::get_power_on_side(world, pos, self.facing.rotate());
        let left_side = RedstoneComparator::get_power_on_side(world, pos, self.facing.rotate_ccw());
        cmp::max(right_side, left_side)
    }

    fn calculate_input_strength(self, world: &impl World, pos: BlockPos) -> u8 {
        let base_input_strength = diode_get_input_strength(world, pos, self.facing);
        let input_pos = pos.offset(self.facing.block_face());
        let input_block = world.get_block(input_pos);
        if input_block.has_comparator_override() {
            input_block.get_comparator_override(world, input_pos)
        } else if base_input_strength < 15 && input_block.is_solid() {
            let far_input_pos = input_pos.offset(self.facing.block_face());
            let far_input_block = world.get_block(far_input_pos);
            if far_input_block.has_comparator_override() {
                far_input_block.get_comparator_override(world, far_input_pos)
            } else {
                base_input_strength
            }
        } else {
            base_input_strength
        }
    }

    fn get_power_on_sides(self, world: &impl World, pos: BlockPos) -> u8 {
        cmp::max(
            RedstoneComparator::get_power_on_side(world, pos, self.facing.rotate()),
            RedstoneComparator::get_power_on_side(world, pos, self.facing.rotate_ccw()),
        )
    }

    pub fn should_be_powered(self, world: &impl World, pos: BlockPos) -> bool {
        let input_strength = self.calculate_input_strength(world, pos);
        if input_strength == 0 {
            false
        } else {
            let power_on_sides = self.get_power_on_sides(world, pos);
            if input_strength > power_on_sides {
                true
            } else {
                power_on_sides == input_strength && self.mode == ComparatorMode::Compare
            }
        }
    }

    fn calculate_output_strength(self, world: &mut impl World, pos: BlockPos) -> u8 {
        let input_strength = Self::calculate_input_strength(self, world, pos);
        if self.mode == ComparatorMode::Subtract {
            input_strength.saturating_sub(self.max_power_on_sides(world, pos))
        } else if input_strength >= self.max_power_on_sides(world, pos) {
            input_strength
        } else {
            0
        }
    }

    // This is exactly the same as it is in the RedstoneRepeater struct.
    // Sometime in the future, this needs to be reused. LLVM might optimize
    // it way, but te human brane wil not!
    fn on_state_change(self, world: &mut impl World, pos: BlockPos) {
        let front_pos = pos.offset(self.facing.opposite().block_face());
        let front_block = world.get_block(front_pos);
        front_block.update(world, front_pos);
        for direction in &BlockFace::values() {
            let neighbor_pos = front_pos.offset(*direction);
            let block = world.get_block(neighbor_pos);
            block.update(world, neighbor_pos);
        }
    }

    pub fn update(self, world: &mut impl World, pos: BlockPos) {
        if world.pending_tick_at(pos) {
            return;
        }
        let output_strength = self.calculate_output_strength(world, pos);
        let old_strength = if let Some(BlockEntity::Comparator { output_strength }) =
            world.get_block_entity(pos)
        {
            *output_strength
        } else {
            0
        };
        if output_strength != old_strength || self.powered != self.should_be_powered(world, pos) {
            let front_block = world.get_block(pos.offset(self.facing.opposite().block_face()));
            let priority = if front_block.is_diode() {
                TickPriority::High
            } else {
                TickPriority::Normal
            };
            world.schedule_tick(pos, 1, priority);
        }
    }

    pub fn tick(mut self, world: &mut impl World, pos: BlockPos) {
        let new_strength = self.calculate_output_strength(world, pos);
        let old_strength = if let Some(BlockEntity::Comparator {
            output_strength: old_output_strength,
        }) = world.get_block_entity(pos)
        {
            *old_output_strength
        } else {
            0
        };
        if new_strength != old_strength || self.mode == ComparatorMode::Compare {
            world.set_block_entity(
                pos,
                BlockEntity::Comparator {
                    output_strength: new_strength,
                },
            );
            let should_be_powered = self.should_be_powered(world, pos);
            let powered = self.powered;
            if powered && !should_be_powered {
                self.powered = false;
                world.set_block(pos, Block::RedstoneComparator { comparator: self });
            } else if !powered && should_be_powered {
                self.powered = true;
                world.set_block(pos, Block::RedstoneComparator { comparator: self });
            }
            self.on_state_change(world, pos);
        }
    }
}

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
pub enum LeverFace {
    Floor,
    #[default]
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
}

impl FromStr for LeverFace {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "floor" => LeverFace::Floor,
            "ceiling" => LeverFace::Ceiling,
            "wall" => LeverFace::Wall,
            _ => return Err(()),
        })
    }
}

impl ToString for LeverFace {
    fn to_string(&self) -> String {
        match self {
            LeverFace::Floor => "floor".to_owned(),
            LeverFace::Ceiling => "ceiling".to_owned(),
            LeverFace::Wall => "wall".to_owned(),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, BlockProperty, BlockTransform)]
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

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
pub enum ButtonFace {
    Floor,
    #[default]
    Wall,
    Ceiling,
}

impl ButtonFace {
    pub(super) fn from_id(id: u32) -> ButtonFace {
        match id {
            0 => ButtonFace::Floor,
            1 => ButtonFace::Wall,
            2 => ButtonFace::Ceiling,
            _ => panic!("Invalid ButtonFace"),
        }
    }

    pub(super) fn get_id(self) -> u32 {
        match self {
            ButtonFace::Floor => 0,
            ButtonFace::Wall => 1,
            ButtonFace::Ceiling => 2,
        }
    }
}

impl FromStr for ButtonFace {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "floor" => ButtonFace::Floor,
            "ceiling" => ButtonFace::Ceiling,
            "wall" => ButtonFace::Wall,
            _ => return Err(()),
        })
    }
}

impl ToString for ButtonFace {
    fn to_string(&self) -> String {
        match self {
            ButtonFace::Floor => "floor".to_owned(),
            ButtonFace::Ceiling => "ceiling".to_owned(),
            ButtonFace::Wall => "wall".to_owned(),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, BlockProperty, BlockTransform)]
pub struct StoneButton {
    pub face: ButtonFace,
    pub facing: BlockDirection,
    pub powered: bool,
}

impl StoneButton {
    pub(super) fn new(face: ButtonFace, facing: BlockDirection, powered: bool) -> StoneButton {
        StoneButton {
            face,
            facing,
            powered,
        }
    }
}
