mod turbo;

use crate::interaction::ActionResult;
use crate::world::World;
use mchprs_blocks::blocks::{Block, RedstoneWire, RedstoneWireSide};
use mchprs_blocks::{BlockDirection, BlockFace, BlockPos};
use turbo::RedstoneWireTurbo;

fn make_cross(power: u8) -> RedstoneWire {
    RedstoneWire {
        north: RedstoneWireSide::Side,
        south: RedstoneWireSide::Side,
        east: RedstoneWireSide::Side,
        west: RedstoneWireSide::Side,
        power,
    }
}

pub fn get_state_for_placement(world: &impl World, pos: BlockPos) -> RedstoneWire {
    let mut wire = RedstoneWire {
        power: calculate_power(world, pos),
        ..Default::default()
    };
    wire = get_regulated_sides(wire, world, pos);
    if is_dot(wire) {
        wire = make_cross(wire.power);
    }
    wire
}

pub fn on_neighbor_changed(
    mut wire: RedstoneWire,
    world: &impl World,
    pos: BlockPos,
    side: BlockFace,
) -> RedstoneWire {
    let old_state = wire;
    let new_side;
    match side {
        BlockFace::Top => return wire,
        BlockFace::Bottom => {
            return get_regulated_sides(wire, world, pos);
        }
        BlockFace::North => {
            wire.south = get_side(world, pos, BlockDirection::South);
            new_side = wire.south;
        }
        BlockFace::South => {
            wire.north = get_side(world, pos, BlockDirection::North);
            new_side = wire.north;
        }

        BlockFace::East => {
            wire.west = get_side(world, pos, BlockDirection::West);
            new_side = wire.west;
        }
        BlockFace::West => {
            wire.east = get_side(world, pos, BlockDirection::East);
            new_side = wire.east;
        }
    }
    wire = get_regulated_sides(wire, world, pos);
    if is_cross(old_state) && new_side.is_none() {
        // Don't mess up the cross
        return old_state;
    }
    if !is_dot(old_state) && is_dot(wire) {
        // Save the power until the transformation into cross is complete
        let power = wire.power;
        // Become the cross it always wanted to be
        wire = make_cross(power);
    }
    wire
}

pub fn on_neighbor_updated(mut wire: RedstoneWire, world: &mut impl World, pos: BlockPos) {
    let new_power = calculate_power(world, pos);

    if wire.power != new_power {
        wire.power = new_power;
        world.set_block(pos, Block::RedstoneWire { wire });
        RedstoneWireTurbo::update_surrounding_neighbors(world, pos);
    }
}

pub fn on_use(wire: RedstoneWire, world: &mut impl World, pos: BlockPos) -> ActionResult {
    if is_dot(wire) || is_cross(wire) {
        let mut new_wire = if is_cross(wire) {
            RedstoneWire::default()
        } else {
            make_cross(0)
        };
        new_wire.power = wire.power;
        new_wire = get_regulated_sides(new_wire, world, pos);
        if wire != new_wire {
            world.set_block(pos, Block::RedstoneWire { wire: new_wire });
            super::update_wire_neighbors(world, pos);
            return ActionResult::Success;
        }
    }
    ActionResult::Pass
}

fn can_connect_to(block: Block, side: BlockDirection) -> bool {
    match block {
        Block::RedstoneWire { .. }
        | Block::RedstoneComparator { .. }
        | Block::RedstoneTorch { .. }
        | Block::RedstoneBlock { .. }
        | Block::RedstoneWallTorch { .. }
        | Block::StonePressurePlate { .. }
        | Block::TripwireHook { .. }
        | Block::StoneButton { .. }
        | Block::Target { .. }
        | Block::Lever { .. } => true,
        Block::RedstoneRepeater { repeater } => {
            repeater.facing == side || repeater.facing == side.opposite()
        }
        Block::Observer { facing } => facing == side.block_facing(),
        _ => false,
    }
}

fn can_connect_diagonal_to(block: Block) -> bool {
    matches!(block, Block::RedstoneWire { .. })
}

pub fn get_current_side(wire: RedstoneWire, side: BlockDirection) -> RedstoneWireSide {
    use BlockDirection::*;
    match side {
        North => wire.north,
        South => wire.south,
        East => wire.east,
        West => wire.west,
    }
}

pub fn get_side(world: &impl World, pos: BlockPos, side: BlockDirection) -> RedstoneWireSide {
    let neighbor_pos = pos.offset(side.block_face());
    let neighbor = world.get_block(neighbor_pos);

    if can_connect_to(neighbor, side) {
        return RedstoneWireSide::Side;
    }

    let up_pos = pos.offset(BlockFace::Top);
    let up = world.get_block(up_pos);

    if !up.is_solid()
        && can_connect_diagonal_to(world.get_block(neighbor_pos.offset(BlockFace::Top)))
    {
        RedstoneWireSide::Up
    } else if !neighbor.is_solid()
        && can_connect_diagonal_to(world.get_block(neighbor_pos.offset(BlockFace::Bottom)))
    {
        RedstoneWireSide::Side
    } else {
        RedstoneWireSide::None
    }
}

fn get_all_sides(mut wire: RedstoneWire, world: &impl World, pos: BlockPos) -> RedstoneWire {
    wire.north = get_side(world, pos, BlockDirection::North);
    wire.south = get_side(world, pos, BlockDirection::South);
    wire.east = get_side(world, pos, BlockDirection::East);
    wire.west = get_side(world, pos, BlockDirection::West);
    wire
}

pub fn get_regulated_sides(wire: RedstoneWire, world: &impl World, pos: BlockPos) -> RedstoneWire {
    let mut state = get_all_sides(wire, world, pos);
    if is_dot(wire) && is_dot(state) {
        return state;
    }
    let north_none = state.north.is_none();
    let south_none = state.south.is_none();
    let east_none = state.east.is_none();
    let west_none = state.west.is_none();
    let north_south_none = north_none && south_none;
    let east_west_none = east_none && west_none;
    if north_none && east_west_none {
        state.north = RedstoneWireSide::Side;
    }
    if south_none && east_west_none {
        state.south = RedstoneWireSide::Side;
    }
    if east_none && north_south_none {
        state.east = RedstoneWireSide::Side;
    }
    if west_none && north_south_none {
        state.west = RedstoneWireSide::Side;
    }
    state
}

fn is_dot(wire: RedstoneWire) -> bool {
    wire.north == RedstoneWireSide::None
        && wire.south == RedstoneWireSide::None
        && wire.east == RedstoneWireSide::None
        && wire.west == RedstoneWireSide::None
}

fn is_cross(wire: RedstoneWire) -> bool {
    wire.north == RedstoneWireSide::Side
        && wire.south == RedstoneWireSide::Side
        && wire.east == RedstoneWireSide::Side
        && wire.west == RedstoneWireSide::Side
}

fn max_wire_power(wire_power: u8, world: &impl World, pos: BlockPos) -> u8 {
    let block = world.get_block(pos);
    if let Block::RedstoneWire { wire } = block {
        wire_power.max(wire.power)
    } else {
        wire_power
    }
}

fn calculate_power(world: &impl World, pos: BlockPos) -> u8 {
    let mut block_power = 0;
    let mut wire_power = 0;

    let up_pos = pos.offset(BlockFace::Top);
    let up_block = world.get_block(up_pos);

    for side in &BlockFace::values() {
        let neighbor_pos = pos.offset(*side);
        wire_power = max_wire_power(wire_power, world, neighbor_pos);
        let neighbor = world.get_block(neighbor_pos);
        block_power = block_power.max(super::get_redstone_power_no_dust(
            neighbor,
            world,
            neighbor_pos,
            *side,
        ));
        if side.is_horizontal() {
            if !up_block.is_solid() && !neighbor.is_transparent() {
                wire_power = max_wire_power(wire_power, world, neighbor_pos.offset(BlockFace::Top));
            }

            if !neighbor.is_solid() {
                wire_power =
                    max_wire_power(wire_power, world, neighbor_pos.offset(BlockFace::Bottom));
            }
        }
    }

    block_power.max(wire_power.saturating_sub(1))
}
