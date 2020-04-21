use std::mem;

#[derive(Copy, Clone, Debug, PartialEq)]
enum BlockFacing {
    North,
    South,
    East,
    West,
}

impl BlockFacing {
    fn from_id(id: u32) -> BlockFacing {
        match id {
            0 => BlockFacing::North,
            1 => BlockFacing::South,
            2 => BlockFacing::West,
            3 => BlockFacing::East,
            _ => panic!("Invalid BlockFacing"),
        }
    }
    fn get_id(self) -> u32 {
        match self {
            BlockFacing::North => 0,
            BlockFacing::South => 1,
            BlockFacing::West => 2,
            BlockFacing::East => 3,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum RedstoneWireSide {
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
    facing: BlockFacing,
    locked: bool,
    powered: bool,
}

impl RedstoneRepeater {
    fn new(delay: u8, facing: BlockFacing, locked: bool, powered: bool) -> RedstoneRepeater {
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
    facing: BlockFacing,
    mode: ComparatorMode,
    powered: bool,
}

impl RedstoneComparator {
    fn new(facing: BlockFacing, mode: ComparatorMode, powered: bool) -> RedstoneComparator {
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
            2056..=3351 => {
                let id = id - 2056;
                let west = RedstoneWireSide::from_id(id % 3);
                let south = RedstoneWireSide::from_id(id % 9 / 3);
                let power = id % 144 / 9;
                let north = RedstoneWireSide::from_id(id % 432 / 144);
                let east = RedstoneWireSide::from_id(id / 432);
                Block::RedstoneWire(RedstoneWire::new(north, south, east, west, power as u8))
            }
            3885 => Block::RedstoneTorch(true),
            3886 => Block::RedstoneTorch(false),
            4017..=4080 => {
                let id = id - 4017;
                let powered = (id & 1) == 0;
                let locked = ((id >> 1) & 1) == 0;
                let facing = BlockFacing::from_id((id >> 2) & 3);
                let delay = (id >> 4) as u8 + 1;
                Block::RedstoneRepeater(RedstoneRepeater::new(delay, facing, locked, powered))
            }
            5140 => Block::RedstoneLamp(true),
            5141 => Block::RedstoneLamp(false),
            6142..=6157 => {
                let id = id - 6142;
                let powered = (id & 1) == 0;
                let mode = ComparatorMode::from_id((id >> 1) & 1);
                let facing = BlockFacing::from_id(id >> 2);
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
}

#[test]
fn repeater_id_test() {
    let original =
        Block::RedstoneRepeater(RedstoneRepeater::new(3, BlockFacing::West, true, false));
    let id = original.get_id();
    assert_eq!(id, 4058);
    let new = Block::from_block_state(id);
    assert_eq!(new, original);
}

#[test]
fn comparator_id_test() {
    let original = Block::RedstoneComparator(RedstoneComparator::new(
        BlockFacing::West,
        ComparatorMode::Subtract,
        false,
    ));
    let id = original.get_id();
    assert_eq!(id, 6153);
    let new = Block::from_block_state(id);
    assert_eq!(new, original);
}
