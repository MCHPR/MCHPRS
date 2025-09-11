use mchprs_blocks::block_entities::BlockEntity;
use mchprs_blocks::blocks::{Block, ComparatorMode, RedstoneComparator};
use mchprs_blocks::{BlockDirection, BlockFace, BlockPos};
use mchprs_world::{TickPriority, World};
use tracing::warn;

fn get_power_on_side(world: &impl World, pos: BlockPos, side: BlockDirection) -> u8 {
    let side_pos = pos.offset(side.block_face());
    let side_block = world.get_block(side_pos);
    if super::is_diode(side_block) {
        super::get_weak_power(side_block, world, side_pos, side.block_face(), false)
    } else if let Block::RedstoneWire { wire } = side_block {
        wire.power
    } else if let Block::RedstoneBlock {} = side_block {
        15
    } else {
        0
    }
}

fn get_power_on_sides(comp: RedstoneComparator, world: &impl World, pos: BlockPos) -> u8 {
    std::cmp::max(
        get_power_on_side(world, pos, comp.facing.rotate()),
        get_power_on_side(world, pos, comp.facing.rotate_ccw()),
    )
}

pub fn has_override(block: Block) -> bool {
    matches!(
        block,
        Block::Barrel { .. }
            | Block::Furnace { .. }
            | Block::Hopper { .. }
            | Block::Cauldron { .. }
            | Block::Composter { .. }
            | Block::Cake { .. }
            | Block::EndPortalFrame { eye: true, .. }
    )
}

pub fn get_override(block: Block, world: &impl World, pos: BlockPos) -> u8 {
    match block {
        Block::Barrel { .. } | Block::Furnace { .. } | Block::Hopper { .. } => {
            match world.get_block_entity(pos) {
                Some(BlockEntity::Container {
                    comparator_override,
                    ..
                }) => *comparator_override,
                Some(other) => {
                    warn!("Backing container blockentity type is invalid: {other:?}");
                    0
                }
                // Empty containers may not have any block entity data
                None => 0,
            }
        }
        Block::Cauldron { level } => level,
        Block::Composter { level } => level,
        Block::Cake { bites } => 14 - 2 * bites,
        Block::EndPortalFrame { eye: true, .. } => 15,
        _ => unreachable!("Block does not override comparators"),
    }
}

pub fn get_far_input(world: &impl World, pos: BlockPos, facing: BlockDirection) -> Option<u8> {
    let face = facing.block_face();
    let input_pos = pos.offset(face);
    let input_block = world.get_block(input_pos);
    if !input_block.is_solid() || has_override(input_block) {
        return None;
    }

    let far_input_pos = input_pos.offset(face);
    let far_input_block = world.get_block(far_input_pos);
    if has_override(far_input_block) {
        Some(get_override(far_input_block, world, far_input_pos))
    } else {
        None
    }
}

fn calculate_input_strength(comp: RedstoneComparator, world: &impl World, pos: BlockPos) -> u8 {
    let base_input_strength = super::diode_get_input_strength(world, pos, comp.facing);
    let input_pos = pos.offset(comp.facing.block_face());
    let input_block = world.get_block(input_pos);
    if has_override(input_block) {
        get_override(input_block, world, input_pos)
    } else if base_input_strength < 15 && input_block.is_solid() {
        let far_input_pos = input_pos.offset(comp.facing.block_face());
        let far_input_block = world.get_block(far_input_pos);
        if has_override(far_input_block) {
            get_override(far_input_block, world, far_input_pos)
        } else {
            base_input_strength
        }
    } else {
        base_input_strength
    }
}

pub fn should_be_powered(comp: RedstoneComparator, world: &impl World, pos: BlockPos) -> bool {
    let input_strength = calculate_input_strength(comp, world, pos);
    if input_strength == 0 {
        false
    } else {
        let power_on_sides = get_power_on_sides(comp, world, pos);
        if input_strength > power_on_sides {
            true
        } else {
            power_on_sides == input_strength && comp.mode == ComparatorMode::Compare
        }
    }
}

fn calculate_output_strength(
    comp: RedstoneComparator,
    world: &mut impl World,
    pos: BlockPos,
) -> u8 {
    let input_strength = calculate_input_strength(comp, world, pos);
    if comp.mode == ComparatorMode::Subtract {
        input_strength.saturating_sub(get_power_on_sides(comp, world, pos))
    } else if input_strength >= get_power_on_sides(comp, world, pos) {
        input_strength
    } else {
        0
    }
}

// This is exactly the same as it is in the RedstoneRepeater struct.
// Sometime in the future, this needs to be reused. LLVM might optimize
// it way, but te human brane wil not!
fn on_state_change(comp: RedstoneComparator, world: &mut impl World, pos: BlockPos) {
    let front_pos = pos.offset(comp.facing.opposite().block_face());
    let front_block = world.get_block(front_pos);
    super::update(front_block, world, front_pos);
    for direction in &BlockFace::values() {
        let neighbor_pos = front_pos.offset(*direction);
        let block = world.get_block(neighbor_pos);
        super::update(block, world, neighbor_pos);
    }
}

pub fn update(comp: RedstoneComparator, world: &mut impl World, pos: BlockPos) {
    if world.pending_tick_at(pos) {
        return;
    }
    let output_strength = calculate_output_strength(comp, world, pos);
    let old_strength =
        if let Some(BlockEntity::Comparator { output_strength }) = world.get_block_entity(pos) {
            *output_strength
        } else {
            0
        };
    if output_strength != old_strength || comp.powered != should_be_powered(comp, world, pos) {
        let front_block = world.get_block(pos.offset(comp.facing.opposite().block_face()));
        let priority = if super::is_diode(front_block) {
            TickPriority::High
        } else {
            TickPriority::Normal
        };
        world.schedule_tick(pos, 1, priority);
    }
}

pub fn tick(mut comp: RedstoneComparator, world: &mut impl World, pos: BlockPos) {
    let new_strength = calculate_output_strength(comp, world, pos);
    let old_strength = if let Some(BlockEntity::Comparator {
        output_strength: old_output_strength,
    }) = world.get_block_entity(pos)
    {
        *old_output_strength
    } else {
        0
    };
    if new_strength != old_strength || comp.mode == ComparatorMode::Compare {
        world.set_block_entity(
            pos,
            BlockEntity::Comparator {
                output_strength: new_strength,
            },
        );
        let should_be_powered = should_be_powered(comp, world, pos);
        let powered = comp.powered;
        if powered && !should_be_powered {
            comp.powered = false;
            world.set_block(pos, Block::RedstoneComparator { comparator: comp });
        } else if !powered && should_be_powered {
            comp.powered = true;
            world.set_block(pos, Block::RedstoneComparator { comparator: comp });
        }
        on_state_change(comp, world, pos);
    }
}
