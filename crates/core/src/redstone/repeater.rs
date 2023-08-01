use crate::world::World;
use mchprs_blocks::blocks::{Block, RedstoneRepeater};
use mchprs_blocks::{BlockDirection, BlockFace, BlockPos};
use mchprs_world::TickPriority;

pub fn get_state_for_placement(
    world: &impl World,
    pos: BlockPos,
    facing: BlockDirection,
) -> RedstoneRepeater {
    RedstoneRepeater {
        delay: 1,
        facing,
        locked: should_be_locked(facing, world, pos),
        powered: false,
    }
}

fn should_be_locked(facing: BlockDirection, world: &impl World, pos: BlockPos) -> bool {
    let right_side = get_power_on_side(world, pos, facing.rotate());
    let left_side = get_power_on_side(world, pos, facing.rotate_ccw());
    std::cmp::max(right_side, left_side) > 0
}

fn get_power_on_side(world: &impl World, pos: BlockPos, side: BlockDirection) -> u8 {
    let side_pos = pos.offset(side.block_face());
    let side_block = world.get_block(side_pos);
    if super::is_diode(side_block) {
        super::get_weak_power(side_block, world, side_pos, side.block_face(), false)
    } else {
        0
    }
}

fn on_state_change(rep: RedstoneRepeater, world: &mut impl World, pos: BlockPos) {
    let front_pos = pos.offset(rep.facing.opposite().block_face());
    let front_block = world.get_block(front_pos);
    super::update(front_block, world, front_pos);
    for direction in &BlockFace::values() {
        let neighbor_pos = front_pos.offset(*direction);
        let block = world.get_block(neighbor_pos);
        super::update(block, world, neighbor_pos);
    }
}

fn schedule_tick(
    rep: RedstoneRepeater,
    world: &mut impl World,
    pos: BlockPos,
    should_be_powered: bool,
) {
    let front_block = world.get_block(pos.offset(rep.facing.opposite().block_face()));
    let priority = if super::is_diode(front_block) {
        TickPriority::Highest
    } else if !should_be_powered {
        TickPriority::Higher
    } else {
        TickPriority::High
    };
    world.schedule_tick(pos, rep.delay as u32, priority);
}

fn should_be_powered(rep: RedstoneRepeater, world: &impl World, pos: BlockPos) -> bool {
    super::diode_get_input_strength(world, pos, rep.facing) > 0
}

pub fn on_neighbor_updated(mut rep: RedstoneRepeater, world: &mut impl World, pos: BlockPos) {
    let should_be_locked = should_be_locked(rep.facing, world, pos);
    if !rep.locked && should_be_locked {
        rep.locked = true;
        world.set_block(pos, Block::RedstoneRepeater { repeater: rep });
    } else if rep.locked && !should_be_locked {
        rep.locked = false;
        world.set_block(pos, Block::RedstoneRepeater { repeater: rep });
    }

    if !rep.locked && !world.pending_tick_at(pos) {
        let should_be_powered = should_be_powered(rep, world, pos);
        if should_be_powered != rep.powered {
            schedule_tick(rep, world, pos, should_be_powered);
        }
    }
}

pub fn tick(mut rep: RedstoneRepeater, world: &mut impl World, pos: BlockPos) {
    if rep.locked {
        return;
    }

    let should_be_powered = should_be_powered(rep, world, pos);
    if rep.powered && !should_be_powered {
        rep.powered = false;
        world.set_block(pos, Block::RedstoneRepeater { repeater: rep });
        on_state_change(rep, world, pos);
    } else if !rep.powered {
        rep.powered = true;
        world.set_block(pos, Block::RedstoneRepeater { repeater: rep });
        on_state_change(rep, world, pos);
    }
}
