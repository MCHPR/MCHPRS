use std::mem;

#[derive(Copy, Clone, Debug)]
enum RedstoneWireSide {
    Up,
    Side,
    None,
}

impl From<u32> for RedstoneWireSide {
    fn from(id: u32) -> RedstoneWireSide {
        match id {
            0 => RedstoneWireSide::Up,
            1 => RedstoneWireSide::Side,
            2 => RedstoneWireSide::None,
            _ => panic!("Invalid RedstoneWireSide"),
        }
    }
}

impl RedstoneWireSide {
    fn get_id(self) -> u32 {
        match self {
            RedstoneWireSide::Up => 0,
            RedstoneWireSide::Side => 1,
            RedstoneWireSide::None => 2,
        }
    }
}

#[derive(Copy, Clone, Debug)]
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

#[derive(Copy, Clone, Debug)]
pub enum Block {
    Air,
    RedstoneWire(RedstoneWire),
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
                let west = RedstoneWireSide::from(id % 3);
                let south = RedstoneWireSide::from(id % 9 / 3);
                let power = id % 144 / 9;
                let north = RedstoneWireSide::from(id % 432 / 144);
                let east = RedstoneWireSide::from(id / 432);
                Block::RedstoneWire(RedstoneWire::new(north, south, east, west, power as u8))
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
            _ => None,
        }
    }
}
