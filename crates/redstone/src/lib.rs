//! A very basic redstone implementation with focus on accuracy over speed.
//! This is the implementation that is used by default in low-performance
//! scenerio (i.e. regular buiding)

pub mod comparator;
pub mod noteblock;
pub mod repeater;
pub mod wire;

use mchprs_blocks::block_entities::BlockEntity;
use mchprs_blocks::blocks::{Block, LeverFace, RedstoneWire};
use mchprs_blocks::{BlockDirection, BlockFace, BlockPos};
use mchprs_world::{TickPriority, World};

pub fn bool_to_ss(b: bool) -> u8 {
    match b {
        true => 15,
        false => 0,
    }
}

fn get_weak_power(
    block: Block,
    world: &impl World,
    pos: BlockPos,
    side: BlockFace,
    dust_power: bool,
) -> u8 {
    match block {
        Block::RedstoneTorch { lit: true } if side != BlockFace::Top => 15,
        Block::RedstoneWallTorch { lit: true, facing } if facing.block_face() != side => 15,
        Block::RedstoneBlock {} => 15,
        Block::StonePressurePlate { powered: true } => 15,
        Block::Lever { powered, .. } if powered => 15,
        Block::StoneButton { powered, .. } if powered => 15,
        Block::Repeater(repeater) if repeater.facing.block_face() == side && repeater.powered => 15,
        Block::Comparator(comparator) if comparator.facing.block_face() == side => {
            if let Some(BlockEntity::Comparator { output_strength }) = world.get_block_entity(pos) {
                *output_strength
            } else {
                0
            }
        }
        Block::RedstoneWire(wire) if dust_power => match side {
            BlockFace::Top => wire.power,
            BlockFace::Bottom => 0,
            _ => {
                let direction = side.unwrap_direction();
                if wire::get_current_side(
                    wire::get_regulated_sides(wire, world, pos),
                    direction.opposite(),
                )
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
    block: Block,
    world: &impl World,
    pos: BlockPos,
    side: BlockFace,
    dust_power: bool,
) -> u8 {
    match block {
        Block::RedstoneTorch { lit: true } if side == BlockFace::Bottom => 15,
        Block::RedstoneWallTorch { lit: true, .. } if side == BlockFace::Bottom => 15,
        Block::Lever {
            face,
            facing,
            powered,
        } => bool_to_ss(
            match side {
                BlockFace::Top => face == LeverFace::Floor,
                BlockFace::Bottom => face == LeverFace::Ceiling,
                _ => face == LeverFace::Wall && facing == side.unwrap_direction(),
            } && powered,
        ),
        Block::StoneButton {
            face,
            facing,
            powered,
        } => bool_to_ss(
            match side {
                BlockFace::Top => face == LeverFace::Floor,
                BlockFace::Bottom => face == LeverFace::Ceiling,
                _ => face == LeverFace::Wall && facing == side.unwrap_direction(),
            } && powered,
        ),
        Block::StonePressurePlate { powered: true } if side == BlockFace::Top => 15,
        Block::RedstoneWire { .. } => get_weak_power(block, world, pos, side, dust_power),
        Block::Repeater(_) => get_weak_power(block, world, pos, side, dust_power),
        Block::Comparator(_) => get_weak_power(block, world, pos, side, dust_power),
        _ => 0,
    }
}

fn get_max_strong_power(world: &impl World, pos: BlockPos, dust_power: bool) -> u8 {
    let mut max_power = 0;
    for side in &BlockFace::values() {
        let block = world.get_block(pos.offset(*side));
        max_power = max_power.max(get_strong_power(
            block,
            world,
            pos.offset(*side),
            *side,
            dust_power,
        ));
    }
    max_power
}

pub fn get_redstone_power(
    block: Block,
    world: &impl World,
    pos: BlockPos,
    facing: BlockFace,
) -> u8 {
    if block.is_solid() {
        get_max_strong_power(world, pos, true)
    } else {
        get_weak_power(block, world, pos, facing, true)
    }
}

fn get_redstone_power_no_dust(
    block: Block,
    world: &impl World,
    pos: BlockPos,
    facing: BlockFace,
) -> u8 {
    if block.is_solid() {
        get_max_strong_power(world, pos, false)
    } else {
        get_weak_power(block, world, pos, facing, false)
    }
}

pub fn torch_should_be_off(world: &impl World, pos: BlockPos) -> bool {
    let bottom_pos = pos.offset(BlockFace::Bottom);
    let bottom_block = world.get_block(bottom_pos);
    get_redstone_power(bottom_block, world, bottom_pos, BlockFace::Top) > 0
}

pub fn wall_torch_should_be_off(
    world: &impl World,
    pos: BlockPos,
    direction: BlockDirection,
) -> bool {
    let wall_pos = pos.offset(direction.opposite().block_face());
    let wall_block = world.get_block(wall_pos);
    get_redstone_power(
        wall_block,
        world,
        wall_pos,
        direction.opposite().block_face(),
    ) > 0
}

pub fn redstone_lamp_should_be_lit(world: &impl World, pos: BlockPos) -> bool {
    for face in &BlockFace::values() {
        let neighbor_pos = pos.offset(*face);
        if get_redstone_power(world.get_block(neighbor_pos), world, neighbor_pos, *face) > 0 {
            return true;
        }
    }
    false
}

fn diode_get_input_strength(world: &impl World, pos: BlockPos, facing: BlockDirection) -> u8 {
    let input_pos = pos.offset(facing.block_face());
    let input_block = world.get_block(input_pos);
    let mut power = get_redstone_power(input_block, world, input_pos, facing.block_face());
    if power == 0 {
        if let Block::RedstoneWire(wire) = input_block {
            power = wire.power;
        }
    }
    power
}

pub fn update(block: Block, world: &mut impl World, pos: BlockPos) {
    match block {
        Block::RedstoneWire(wire) => {
            wire::on_neighbor_updated(wire, world, pos);
        }
        Block::RedstoneTorch { lit } => {
            if lit == torch_should_be_off(world, pos) && !world.pending_tick_at(pos) {
                world.schedule_tick(pos, 1, TickPriority::Normal);
            }
        }
        Block::RedstoneWallTorch { lit, facing } => {
            if lit == wall_torch_should_be_off(world, pos, facing) && !world.pending_tick_at(pos) {
                world.schedule_tick(pos, 1, TickPriority::Normal);
            }
        }
        Block::Repeater(repeater) => {
            repeater::on_neighbor_updated(repeater, world, pos);
        }
        Block::Comparator(comparator) => {
            comparator::update(comparator, world, pos);
        }
        Block::RedstoneLamp { lit } => {
            let should_be_lit = redstone_lamp_should_be_lit(world, pos);
            if lit && !should_be_lit {
                world.schedule_tick(pos, 2, TickPriority::Normal);
            } else if !lit && should_be_lit {
                world.set_block(pos, Block::RedstoneLamp { lit: true });
            }
        }
        Block::IronTrapdoor {
            powered,
            facing,
            half,
            open,
            waterlogged,
        } => {
            let should_be_powered = redstone_lamp_should_be_lit(world, pos);
            if powered != should_be_powered {
                let new_block = Block::IronTrapdoor {
                    facing,
                    half,
                    powered: should_be_powered,
                    open,
                    waterlogged,
                };
                world.set_block(pos, new_block);
            }
        }
        Block::NoteBlock {
            instrument: _instrument,
            note,
            ..
        } => {
            let should_be_powered = redstone_lamp_should_be_lit(world, pos);
            // We need to recheck if the live version of the block is powered,
            // because the supplied block is cached and could be outdated
            let Block::NoteBlock { powered, .. } = world.get_block(pos) else {
                unreachable!("Underlying block changed, this should never happen")
            };
            if powered != should_be_powered {
                // Hack: Update the instrument only just before the noteblock is updated
                let instrument = noteblock::get_noteblock_instrument(world, pos);
                let new_block = Block::NoteBlock {
                    instrument,
                    note,
                    powered: should_be_powered,
                };

                if should_be_powered && noteblock::is_noteblock_unblocked(world, pos) {
                    noteblock::play_note(world, pos, instrument, note);
                }
                world.set_block(pos, new_block);
            }
        }
        _ => {}
    }
}

pub fn tick(block: Block, world: &mut impl World, pos: BlockPos) {
    match block {
        Block::Repeater(repeater) => {
            repeater::tick(repeater, world, pos);
        }
        Block::Comparator(comparator) => {
            comparator::tick(comparator, world, pos);
        }
        Block::RedstoneTorch { lit } => {
            let should_be_off = torch_should_be_off(world, pos);
            if lit && should_be_off {
                world.set_block(pos, Block::RedstoneTorch { lit: false });
                update_surrounding_blocks(world, pos);
            } else if !lit && !should_be_off {
                world.set_block(pos, Block::RedstoneTorch { lit: true });
                update_surrounding_blocks(world, pos);
            }
        }
        Block::RedstoneWallTorch { lit, facing } => {
            let should_be_off = wall_torch_should_be_off(world, pos, facing);
            if lit && should_be_off {
                world.set_block(pos, Block::RedstoneWallTorch { lit: false, facing });
                update_surrounding_blocks(world, pos);
            } else if !lit && !should_be_off {
                world.set_block(pos, Block::RedstoneWallTorch { lit: true, facing });
                update_surrounding_blocks(world, pos);
            }
        }
        Block::RedstoneLamp { lit } => {
            let should_be_lit = redstone_lamp_should_be_lit(world, pos);
            if lit && !should_be_lit {
                world.set_block(pos, Block::RedstoneLamp { lit: false });
            }
        }
        Block::StoneButton {
            powered,
            face,
            facing,
        } => {
            if powered {
                world.set_block(
                    pos,
                    Block::StoneButton {
                        face,
                        facing,
                        powered: false,
                    },
                );
                update_surrounding_blocks(world, pos);
                match face {
                    LeverFace::Ceiling => {
                        update_surrounding_blocks(world, pos.offset(BlockFace::Top));
                    }
                    LeverFace::Floor => {
                        update_surrounding_blocks(world, pos.offset(BlockFace::Bottom));
                    }
                    LeverFace::Wall => {
                        update_surrounding_blocks(world, pos.offset(facing.opposite().block_face()))
                    }
                }
            }
        }
        _ => {}
    }
}

pub fn update_wire_neighbors(world: &mut impl World, pos: BlockPos) {
    for direction in &BlockFace::values() {
        let neighbor_pos = pos.offset(*direction);
        let block = world.get_block(neighbor_pos);
        update(block, world, neighbor_pos);
        for n_direction in &BlockFace::values() {
            let n_neighbor_pos = neighbor_pos.offset(*n_direction);
            let block = world.get_block(n_neighbor_pos);
            update(block, world, n_neighbor_pos);
        }
    }
}

pub fn update_surrounding_blocks(world: &mut impl World, pos: BlockPos) {
    for direction in &BlockFace::values() {
        let neighbor_pos = pos.offset(*direction);
        let block = world.get_block(neighbor_pos);
        update(block, world, neighbor_pos);

        // Also update diagonal blocks

        let up_pos = neighbor_pos.offset(BlockFace::Top);
        let up_block = world.get_block(up_pos);
        update(up_block, world, up_pos);

        let down_pos = neighbor_pos.offset(BlockFace::Bottom);
        let down_block = world.get_block(down_pos);
        update(down_block, world, down_pos);
    }
}

pub fn is_diode(block: Block) -> bool {
    matches!(block, Block::Repeater(_) | Block::Comparator(_))
}

/// Returns true if the action was handled
pub fn on_use(block: Block, world: &mut impl World, pos: BlockPos) -> bool {
    match block {
        Block::Repeater(mut repeater) => {
            repeater.delay += 1;
            if repeater.delay > 4 {
                repeater.delay -= 4;
            }
            world.set_block(pos, Block::Repeater(repeater));
            true
        }
        Block::Comparator(comparator) => {
            let mut comparator = comparator;
            comparator.mode = comparator.mode.toggle();
            comparator::tick(comparator, world, pos);
            world.set_block(pos, Block::Comparator(comparator));
            true
        }
        Block::Lever {
            face,
            facing,
            powered,
        } => {
            world.set_block(
                pos,
                Block::Lever {
                    powered: !powered,
                    face,
                    facing,
                },
            );
            update_surrounding_blocks(world, pos);
            match face {
                LeverFace::Ceiling => {
                    update_surrounding_blocks(world, pos.offset(BlockFace::Top));
                }
                LeverFace::Floor => {
                    update_surrounding_blocks(world, pos.offset(BlockFace::Bottom));
                }
                LeverFace::Wall => {
                    update_surrounding_blocks(world, pos.offset(facing.opposite().block_face()))
                }
            }
            true
        }
        Block::StoneButton {
            powered,
            face,
            facing,
        } => {
            if !powered {
                world.set_block(
                    pos,
                    Block::StoneButton {
                        powered: true,
                        face,
                        facing,
                    },
                );
                world.schedule_tick(pos, 10, TickPriority::Normal);
                update_surrounding_blocks(world, pos);
                match face {
                    LeverFace::Ceiling => {
                        update_surrounding_blocks(world, pos.offset(BlockFace::Top));
                    }
                    LeverFace::Floor => {
                        update_surrounding_blocks(world, pos.offset(BlockFace::Bottom));
                    }
                    LeverFace::Wall => {
                        update_surrounding_blocks(world, pos.offset(facing.opposite().block_face()))
                    }
                }
            }
            true
        }
        Block::RedstoneWire(wire) => {
            if wire::is_dot(wire) || wire::is_cross(wire) {
                let mut new_wire = if wire::is_cross(wire) {
                    RedstoneWire::default()
                } else {
                    wire::make_cross(0)
                };
                new_wire.power = wire.power;
                new_wire = wire::get_regulated_sides(new_wire, world, pos);
                if wire != new_wire {
                    world.set_block(pos, Block::RedstoneWire(new_wire));
                    update_wire_neighbors(world, pos);
                    return true;
                }
            }
            false
        }
        Block::NoteBlock { note, powered, .. } => {
            let note = (note + 1) % 25;
            let instrument = noteblock::get_noteblock_instrument(world, pos);

            world.set_block(
                pos,
                Block::NoteBlock {
                    instrument,
                    note,
                    powered,
                },
            );

            if noteblock::is_noteblock_unblocked(world, pos) {
                noteblock::play_note(world, pos, instrument, note);
            }

            true
        }
        _ => false,
    }
}
