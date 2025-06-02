use crate::change_block;
use mchprs_blocks::blocks::{Block, RedstoneObserver};
use mchprs_blocks::{BlockFace, BlockPos};
use mchprs_world::{TickPriority, World};

pub fn on_neighbour_changed(
    obs: RedstoneObserver,
    world: &mut impl World,
    pos: BlockPos,
    source_face: BlockFace
) {
    if source_face == obs.facing.block_face() && !world.pending_tick_at(pos) {
        if !obs.powered {
            world.schedule_tick(pos, 1, TickPriority::Higher);
        }
    }
}

// This is (more or less) the same as it is in the RedstoneRepeater struct.
// Sometime in the future, this needs to be reused. LLVM might optimize
// it way, but te human brane wil not!
fn on_state_change(obs: RedstoneObserver, world: &mut impl World, pos: BlockPos) {
    let back_pos = pos.offset(obs.facing.opposite().block_face());
    let back_block = world.get_block(back_pos);
    super::update(back_block, world, back_pos);
    for direction in &BlockFace::values() {
        let neighbor_pos = back_pos.offset(*direction);
        let block = world.get_block(neighbor_pos);
        super::update(block, world, neighbor_pos);
    }
}

pub fn tick(mut obs: RedstoneObserver, world: &mut impl World, pos: BlockPos) {
    if obs.powered {
        obs.powered = false;
        change_block(world, pos, Block::RedstoneObserver { observer: obs });
    } else {
        obs.powered = true;
        change_block(world, pos, Block::RedstoneObserver { observer: obs });
        world.schedule_tick(pos, 1, TickPriority::Normal);
    }
    on_state_change(obs, world, pos);
}
