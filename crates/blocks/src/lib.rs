pub mod block_entities;
pub mod items;

pub use mchprs_proc_macros::BlockProperty;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

#[derive(PartialEq, Eq, Copy, Clone, Debug, Serialize, Deserialize, Hash)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPos {
    pub const fn new(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos { x, y, z }
    }

    pub fn zero() -> BlockPos {
        BlockPos::new(0, 0, 0)
    }

    pub fn offset(self, face: BlockFace) -> BlockPos {
        match face {
            BlockFace::Bottom => BlockPos::new(self.x, self.y.saturating_sub(1), self.z),
            BlockFace::Top => BlockPos::new(self.x, self.y + 1, self.z),
            BlockFace::North => BlockPos::new(self.x, self.y, self.z - 1),
            BlockFace::South => BlockPos::new(self.x, self.y, self.z + 1),
            BlockFace::West => BlockPos::new(self.x - 1, self.y, self.z),
            BlockFace::East => BlockPos::new(self.x + 1, self.y, self.z),
        }
    }

    pub fn max(self, other: BlockPos) -> BlockPos {
        BlockPos {
            x: std::cmp::max(self.x, other.x),
            y: std::cmp::max(self.y, other.y),
            z: std::cmp::max(self.z, other.z),
        }
    }

    pub fn min(self, other: BlockPos) -> BlockPos {
        BlockPos {
            x: std::cmp::min(self.x, other.x),
            y: std::cmp::min(self.y, other.y),
            z: std::cmp::min(self.z, other.z),
        }
    }
}

impl std::ops::Sub for BlockPos {
    type Output = BlockPos;

    fn sub(self, rhs: BlockPos) -> BlockPos {
        BlockPos {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            z: self.z - rhs.z,
        }
    }
}

impl std::ops::Add for BlockPos {
    type Output = BlockPos;

    fn add(self, rhs: BlockPos) -> BlockPos {
        BlockPos {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
        }
    }
}

impl std::ops::Mul<i32> for BlockPos {
    type Output = BlockPos;

    fn mul(self, rhs: i32) -> BlockPos {
        BlockPos {
            x: self.x * rhs,
            y: self.y * rhs,
            z: self.z * rhs,
        }
    }
}

impl std::fmt::Display for BlockPos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {}, {})", self.x, self.y, self.z)
    }
}

pub trait BlockProperty: Sized {
    fn encode(self, props: &mut HashMap<&'static str, String>, name: &'static str);
    fn decode(&mut self, props: &HashMap<&str, &str>, name: &str);
}

impl<T> BlockProperty for T
where
    T: ToString + FromStr,
{
    fn encode(self, props: &mut HashMap<&'static str, String>, name: &'static str) {
        props.insert(name, self.to_string());
    }

    fn decode(&mut self, props: &HashMap<&str, &str>, name: &str) {
        if let Some(&str) = props.get(name) {
            if let Ok(val) = str.parse() {
                *self = val;
            }
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
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
            _ => BlockFace::West,
        }
    }
}

impl BlockFace {
    pub fn values() -> [BlockFace; 6] {
        use BlockFace::*;
        [Top, Bottom, North, South, East, West]
    }

    pub fn is_horizontal(self) -> bool {
        use BlockFace::*;
        matches!(self, North | South | East | West)
    }

    pub fn to_direction(self) -> BlockDirection {
        match self {
            BlockFace::North => BlockDirection::North,
            BlockFace::South => BlockDirection::South,
            BlockFace::East => BlockDirection::East,
            BlockFace::West => BlockDirection::West,
            _ => BlockDirection::West,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BlockColorVariant {
    White = 0,
    Orange = 1,
    Magenta = 2,
    LightBlue = 3,
    Yellow = 4,
    Lime = 5,
    Pink = 6,
    Gray = 7,
    LightGray = 8,
    Cyan = 9,
    Purple = 10,
    Blue = 11,
    Brown = 12,
    Green = 13,
    Red = 14,
    Black = 15,
}

impl BlockColorVariant {
    pub fn get_id(self) -> u32 {
        self as u32
    }

    pub fn from_id(id: u32) -> BlockColorVariant {
        use BlockColorVariant::*;
        match id {
            0 => White,
            1 => Orange,
            2 => Magenta,
            3 => LightBlue,
            4 => Yellow,
            5 => Lime,
            6 => Pink,
            7 => Gray,
            8 => LightGray,
            9 => Cyan,
            10 => Purple,
            11 => Blue,
            12 => Brown,
            13 => Green,
            14 => Red,
            15 => Black,
            _ => unreachable!(),
        }
    }
}

impl BlockProperty for BlockColorVariant {
    // Don't encode: the color is encoded in the block name
    fn encode(self, _props: &mut HashMap<&'static str, String>, _name: &'static str) {}
    fn decode(&mut self, _props: &HashMap<&str, &str>, _name: &str) {}
}

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
pub enum BlockDirection {
    North,
    South,
    East,
    #[default]
    West,
}

impl BlockDirection {
    pub fn opposite(self) -> BlockDirection {
        use BlockDirection::*;
        match self {
            North => South,
            South => North,
            East => West,
            West => East,
        }
    }

    pub fn block_face(self) -> BlockFace {
        use BlockDirection::*;
        match self {
            North => BlockFace::North,
            South => BlockFace::South,
            East => BlockFace::East,
            West => BlockFace::West,
        }
    }

    pub fn block_facing(self) -> BlockFacing {
        use BlockDirection::*;
        match self {
            North => BlockFacing::North,
            South => BlockFacing::South,
            East => BlockFacing::East,
            West => BlockFacing::West,
        }
    }

    pub fn from_id(id: u32) -> BlockDirection {
        match id {
            0 => BlockDirection::North,
            1 => BlockDirection::South,
            2 => BlockDirection::West,
            3 => BlockDirection::East,
            _ => panic!("Invalid BlockDirection"),
        }
    }

    pub fn get_id(self) -> u32 {
        match self {
            BlockDirection::North => 0,
            BlockDirection::South => 1,
            BlockDirection::West => 2,
            BlockDirection::East => 3,
        }
    }

    pub fn rotate(self) -> BlockDirection {
        use BlockDirection::*;
        match self {
            North => East,
            East => South,
            South => West,
            West => North,
        }
    }

    pub fn rotate_ccw(self) -> BlockDirection {
        use BlockDirection::*;
        match self {
            North => West,
            West => South,
            South => East,
            East => North,
        }
    }
}

impl FromStr for BlockDirection {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "north" => BlockDirection::North,
            "south" => BlockDirection::South,
            "east" => BlockDirection::East,
            "west" => BlockDirection::West,
            _ => return Err(()),
        })
    }
}

impl ToString for BlockDirection {
    fn to_string(&self) -> String {
        match self {
            BlockDirection::North => "north".to_owned(),
            BlockDirection::South => "south".to_owned(),
            BlockDirection::East => "east".to_owned(),
            BlockDirection::West => "west".to_owned(),
        }
    }
}

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
pub enum BlockFacing {
    North,
    East,
    South,
    #[default]
    West,
    Up,
    Down,
}

impl BlockFacing {
    pub fn from_id(id: u32) -> BlockFacing {
        match id {
            0 => BlockFacing::North,
            1 => BlockFacing::East,
            2 => BlockFacing::South,
            3 => BlockFacing::West,
            4 => BlockFacing::Up,
            5 => BlockFacing::Down,
            _ => BlockFacing::West,
        }
    }

    pub fn get_id(self) -> u32 {
        match self {
            BlockFacing::North => 0,
            BlockFacing::East => 1,
            BlockFacing::South => 2,
            BlockFacing::West => 3,
            BlockFacing::Up => 4,
            BlockFacing::Down => 5,
        }
    }

    pub fn offset_pos(self, mut pos: BlockPos, n: i32) -> BlockPos {
        match self {
            BlockFacing::North => pos.z -= n,
            BlockFacing::South => pos.z += n,
            BlockFacing::East => pos.x += n,
            BlockFacing::West => pos.x -= n,
            BlockFacing::Up => pos.y += n,
            BlockFacing::Down => pos.y -= n,
        }
        pos
    }

    pub fn rotate(self) -> BlockFacing {
        use BlockFacing::*;
        match self {
            North => East,
            East => South,
            South => West,
            West => North,
            other => other,
        }
    }

    pub fn rotate_ccw(self) -> BlockFacing {
        use BlockFacing::*;
        match self {
            North => West,
            West => South,
            South => East,
            East => North,
            other => other,
        }
    }
}

impl ToString for BlockFacing {
    fn to_string(&self) -> String {
        match self {
            BlockFacing::North => "north".to_owned(),
            BlockFacing::South => "south".to_owned(),
            BlockFacing::East => "east".to_owned(),
            BlockFacing::West => "west".to_owned(),
            BlockFacing::Up => "up".to_owned(),
            BlockFacing::Down => "down".to_owned(),
        }
    }
}

impl FromStr for BlockFacing {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "north" => BlockFacing::North,
            "south" => BlockFacing::South,
            "east" => BlockFacing::East,
            "west" => BlockFacing::West,
            "up" => BlockFacing::Up,
            "down" => BlockFacing::Down,
            _ => return Err(()),
        })
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct SignType(pub u32);

impl BlockProperty for SignType {
    // Don't encode
    fn encode(self, _props: &mut HashMap<&'static str, String>, _name: &'static str) {}
    fn decode(&mut self, _props: &HashMap<&str, &str>, _name: &str) {}
}
