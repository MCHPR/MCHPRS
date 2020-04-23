use crate::items::{ActionResult, UseOnBlockContext};
use crate::plot::Plot;
use std::mem;
use log::error;

#[derive(PartialEq, Eq, Clone)]
pub struct BlockPos {
    pub x: i32,
    pub y: u32,
    pub z: i32,
}

impl BlockPos {
    pub fn new(x: i32, y: u32, z: i32) -> BlockPos {
        BlockPos { x, y, z }
    }

    pub fn offset(&self, face: BlockFace) -> BlockPos {
        match face {
            BlockFace::Bottom => BlockPos::new(self.x, self.y - 1, self.z),
            BlockFace::Top => BlockPos::new(self.x, self.y + 1, self.z),
            BlockFace::North => BlockPos::new(self.x, self.y, self.z - 1),
            BlockFace::South => BlockPos::new(self.x, self.y, self.z + 1),
            BlockFace::West => BlockPos::new(self.x - 1, self.y, self.z),
            BlockFace::East => BlockPos::new(self.x + 1, self.y, self.z),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum BlockDirection {
    North,
    South,
    East,
    West,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum BlockFace {
    Bottom,
    Top,
    North,
    South,
    West,
    East,
}

impl BlockFace {
    pub fn from_id(id: u32) -> BlockFace {
        match id {
            0 => BlockFace::Bottom,
            1 => BlockFace::Top,
            2 => BlockFace::North,
            3 => BlockFace::South,
            4 => BlockFace::West,
            5 => BlockFace::East,
            _ => panic!("Invalid BlockFace"),
        }
    }
}

impl BlockDirection {
    fn from_id(id: u32) -> BlockDirection {
        match id {
            0 => BlockDirection::North,
            1 => BlockDirection::South,
            2 => BlockDirection::West,
            3 => BlockDirection::East,
            _ => panic!("Invalid BlockDirection"),
        }
    }
    fn get_id(self) -> u32 {
        match self {
            BlockDirection::North => 0,
            BlockDirection::South => 1,
            BlockDirection::West => 2,
            BlockDirection::East => 3,
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
    fn from_id(id: u32) -> RedstoneWireSide {
        match id {
            0 => RedstoneWireSide::Up,
            1 => RedstoneWireSide::Side,
            2 => RedstoneWireSide::None,
            _ => panic!("Invalid RedstoneWireSide"),
        }
    }
    fn get_id(self) -> u32 {
        match self {
            RedstoneWireSide::Up => 0,
            RedstoneWireSide::Side => 1,
            RedstoneWireSide::None => 2,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct RedstoneWire {
    north: RedstoneWireSide,
    south: RedstoneWireSide,
    east: RedstoneWireSide,
    west: RedstoneWireSide,
    power: u8,
}

impl RedstoneWire {
    fn new(
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
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct RedstoneRepeater {
    delay: u8,
    facing: BlockDirection,
    locked: bool,
    powered: bool,
}

impl RedstoneRepeater {
    fn new(delay: u8, facing: BlockDirection, locked: bool, powered: bool) -> RedstoneRepeater {
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
    fn from_id(id: u32) -> ComparatorMode {
        match id {
            0 => ComparatorMode::Compare,
            1 => ComparatorMode::Subtract,
            _ => panic!("Invalid ComparatorMode"),
        }
    }
    fn get_id(self) -> u32 {
        match self {
            ComparatorMode::Compare => 0,
            ComparatorMode::Subtract => 1,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct RedstoneComparator {
    facing: BlockDirection,
    mode: ComparatorMode,
    powered: bool,
}

impl RedstoneComparator {
    fn new(facing: BlockDirection, mode: ComparatorMode, powered: bool) -> RedstoneComparator {
        RedstoneComparator {
            facing,
            mode,
            powered,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Block {
    Air,
    RedstoneWire(RedstoneWire),
    RedstoneRepeater(RedstoneRepeater),
    RedstoneComparator(RedstoneComparator),
    RedstoneTorch(bool),
    RedstoneWallTorch(bool, BlockDirection),
    RedstoneLamp(bool),
    Solid(u32),
    Transparent(u32),
}

impl Block {
    pub fn compare_variant(&self, other: &Block) -> bool {
        mem::discriminant(self) == mem::discriminant(other)
    }

    pub fn from_block_state(id: u32) -> Block {
        match id {
            0 => Block::Air,
            // Redstone Wire
            2056..=3351 => {
                let id = id - 2056;
                let west = RedstoneWireSide::from_id(id % 3);
                let south = RedstoneWireSide::from_id(id % 9 / 3);
                let power = id % 144 / 9;
                let north = RedstoneWireSide::from_id(id % 432 / 144);
                let east = RedstoneWireSide::from_id(id / 432);
                Block::RedstoneWire(RedstoneWire::new(north, south, east, west, power as u8))
            }
            // Redstone Torch
            3885 => Block::RedstoneTorch(true),
            3886 => Block::RedstoneTorch(false),
            // Redstone Wall Torch
            3887..=3894 => {
                let id = id - 3887;
                let lit = (id & 1) == 0;
                let facing = BlockDirection::from_id(id >> 1);
                Block::RedstoneWallTorch(lit, facing) 
            }
            // Redstone Repeater
            4017..=4080 => {
                let id = id - 4017;
                let powered = (id & 1) == 0;
                let locked = ((id >> 1) & 1) == 0;
                let facing = BlockDirection::from_id((id >> 2) & 3);
                let delay = (id >> 4) as u8 + 1;
                Block::RedstoneRepeater(RedstoneRepeater::new(delay, facing, locked, powered))
            }
            // Redstone Lamp
            5140 => Block::RedstoneLamp(true),
            5141 => Block::RedstoneLamp(false),
            // Redstone Comparator
            6142..=6157 => {
                let id = id - 6142;
                let powered = (id & 1) == 0;
                let mode = ComparatorMode::from_id((id >> 1) & 1);
                let facing = BlockDirection::from_id(id >> 2);
                Block::RedstoneComparator(RedstoneComparator::new(facing, mode, powered))
            }
            _ => Block::Solid(id),
        }
    }

    pub fn get_id(self) -> u32 {
        match self {
            Block::Air => 0,
            Block::RedstoneWire(wire) => {
                wire.east.get_id() * 432
                    + wire.north.get_id() * 144
                    + wire.power as u32 * 9
                    + wire.south.get_id() * 3
                    + wire.west.get_id()
                    + 2056
            }
            Block::RedstoneTorch(true) => 3885,
            Block::RedstoneTorch(false) => 3886,
            Block::RedstoneWallTorch(lit, facing) => {
                (facing.get_id() << 1)
                    + (!lit as u32)
                    + 3887
            }
            Block::RedstoneRepeater(repeater) => {
                (repeater.delay as u32 - 1) * 16
                    + repeater.facing.get_id() * 4
                    + !repeater.locked as u32 * 2
                    + !repeater.powered as u32
                    + 4017
            }
            Block::RedstoneLamp(true) => 5140,
            Block::RedstoneLamp(false) => 5141,
            Block::RedstoneComparator(comparator) => {
                comparator.facing.get_id() * 4
                    + comparator.mode.get_id() * 2
                    + !comparator.powered as u32
                    + 6142
            }
            Block::Solid(id) => id,
            Block::Transparent(id) => id,
        }
    }

    pub fn from_name(name: &str) -> Option<Block> {
        match name {
            "air" => Some(Block::Air),
            "glass" => Some(Block::Transparent(230)),
            "sandstone" => Some(Block::Solid(245)),
            "stone_bricks" => Some(Block::Solid(4481)),
            _ => None,
        }
    }

    pub fn on_use(&self, plot: &mut Plot, pos: &BlockPos) -> ActionResult {
        match self {
            Block::RedstoneRepeater(repeater) => {
                let mut repeater = repeater.clone();
                repeater.delay += 1;
                if repeater.delay > 4 {
                    repeater.delay -= 4;
                }
                plot.set_block(&pos, Block::RedstoneRepeater(repeater));
                ActionResult::Success
            }
            _ => ActionResult::Pass,
        }
    }

    pub fn get_block_for_placement(item_id: u32, context: &UseOnBlockContext) -> Block {
        match item_id {
            // Glass
            64 => Block::Transparent(230),
            // Sandstone
            68 => Block::Solid(245),
            // Wool
            82..=97 => Block::Solid(item_id + 1301),
            173 => match context.block_face {
                BlockFace::Top => Block::RedstoneTorch(true),
                BlockFace::Bottom => Block::Air,
                BlockFace::North => Block::RedstoneWallTorch(true, BlockDirection::North),
                BlockFace::South => Block::RedstoneWallTorch(true, BlockDirection::South),
                BlockFace::East => Block::RedstoneWallTorch(true, BlockDirection::East),
                BlockFace::West => Block::RedstoneWallTorch(true, BlockDirection::West),
            },
            // Concrete
            413..=428 => Block::Solid(item_id + 8489),
            // Redstone Repeater
            513 => Block::RedstoneRepeater(RedstoneRepeater {
                delay: 1,
                facing: context.player_direction,
                locked: false,
                powered: false,
            }),
            _ => {
                error!("Tried to place block which wasnt a block!");
                Block::Solid(245)
            }
        }
    }

    pub fn place_in_plot(self, plot: &mut Plot, pos: &BlockPos) {
        match self {
            Block::RedstoneRepeater(_) => {
                // TODO: Queue repeater tick
                plot.set_block(pos, self);
            }
            _ => {
                plot.set_block(pos, self);
            }
        }
    }
}

#[test]
fn repeater_id_test() {
    let original =
        Block::RedstoneRepeater(RedstoneRepeater::new(3, BlockDirection::West, true, false));
    let id = original.get_id();
    assert_eq!(id, 4058);
    let new = Block::from_block_state(id);
    assert_eq!(new, original);
}

#[test]
fn comparator_id_test() {
    let original = Block::RedstoneComparator(RedstoneComparator::new(
        BlockDirection::West,
        ComparatorMode::Subtract,
        false,
    ));
    let id = original.get_id();
    assert_eq!(id, 6153);
    let new = Block::from_block_state(id);
    assert_eq!(new, original);
}
