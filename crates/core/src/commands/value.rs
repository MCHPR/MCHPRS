use crate::{
    commands::error::{CommandResult, InternalError},
    worldedit::WorldEditPattern,
};
use mchprs_blocks::{block_entities::ContainerType, BlockFacing, BlockPos};
use rustc_hash::FxHashSet;

#[derive(Debug, Clone, Copy)]
pub enum RelativeCoord<T> {
    Absolute(T),
    Relative(T),
}

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Me,
    Left,
    Right,
    Up,
    Down,
    North,
    South,
    East,
    West,
}

impl Direction {
    pub fn resolve(&self, player_facing: BlockFacing) -> BlockFacing {
        match self {
            Direction::Up => BlockFacing::Up,
            Direction::Down => BlockFacing::Down,
            Direction::North => BlockFacing::North,
            Direction::South => BlockFacing::South,
            Direction::East => BlockFacing::East,
            Direction::West => BlockFacing::West,
            Direction::Me => player_facing,
            Direction::Left => player_facing.rotate_ccw(),
            Direction::Right => player_facing.rotate(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum DirectionExt {
    Me,
    Left,
    Right,
    Up,
    Down,
    North,
    South,
    East,
    West,
    LeftUp,
    LeftDown,
    RightUp,
    RightDown,
    NorthUp,
    NorthDown,
    SouthUp,
    SouthDown,
    EastUp,
    EastDown,
    WestUp,
    WestDown,
}

impl DirectionExt {
    pub fn resolve(&self, player_facing: BlockFacing, player_pitch: f32) -> BlockPos {
        match self {
            DirectionExt::Up => BlockPos::new(0, 1, 0),
            DirectionExt::Down => BlockPos::new(0, -1, 0),
            DirectionExt::North => BlockPos::new(0, 0, -1),
            DirectionExt::South => BlockPos::new(0, 0, 1),
            DirectionExt::East => BlockPos::new(1, 0, 0),
            DirectionExt::West => BlockPos::new(-1, 0, 0),
            DirectionExt::NorthUp => BlockPos::new(0, 1, -1),
            DirectionExt::NorthDown => BlockPos::new(0, -1, -1),
            DirectionExt::SouthUp => BlockPos::new(0, 1, 1),
            DirectionExt::SouthDown => BlockPos::new(0, -1, 1),
            DirectionExt::EastUp => BlockPos::new(1, 1, 0),
            DirectionExt::EastDown => BlockPos::new(1, -1, 0),
            DirectionExt::WestUp => BlockPos::new(-1, 1, 0),
            DirectionExt::WestDown => BlockPos::new(-1, -1, 0),
            DirectionExt::Me => {
                let mut vec = player_facing.offset_pos(BlockPos::zero(), 1);
                if !matches!(player_facing, BlockFacing::Down | BlockFacing::Up) {
                    if player_pitch > 22.5 {
                        vec.y -= 1;
                    } else if player_pitch < -22.5 {
                        vec.y += 1;
                    }
                }
                vec
            }
            DirectionExt::Left => player_facing.rotate_ccw().offset_pos(BlockPos::zero(), 1),
            DirectionExt::Right => player_facing.rotate().offset_pos(BlockPos::zero(), 1),
            DirectionExt::LeftUp => player_facing
                .rotate_ccw()
                .offset_pos(BlockPos::new(0, 1, 0), 1),
            DirectionExt::LeftDown => player_facing
                .rotate_ccw()
                .offset_pos(BlockPos::new(0, -1, 0), 1),
            DirectionExt::RightUp => player_facing.rotate().offset_pos(BlockPos::new(0, 1, 0), 1),
            DirectionExt::RightDown => player_facing
                .rotate()
                .offset_pos(BlockPos::new(0, -1, 0), 1),
        }
    }
}

impl<T> RelativeCoord<T>
where
    T: std::ops::Add<Output = T> + Copy,
{
    pub fn resolve(&self, reference: T) -> T {
        match self {
            RelativeCoord::Absolute(val) => *val,
            RelativeCoord::Relative(offset) => reference + *offset,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Vec3 {
    pub x: RelativeCoord<f64>,
    pub y: RelativeCoord<f64>,
    pub z: RelativeCoord<f64>,
}

impl Vec3 {
    pub fn resolve(&self, reference: (f64, f64, f64)) -> (f64, f64, f64) {
        (
            self.x.resolve(reference.0),
            self.y.resolve(reference.1),
            self.z.resolve(reference.2),
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ColumnPos {
    pub x: RelativeCoord<i32>,
    pub z: RelativeCoord<i32>,
}

impl ColumnPos {
    pub fn resolve(&self, reference: (i32, i32)) -> (i32, i32) {
        (self.x.resolve(reference.0), self.z.resolve(reference.1))
    }
}

#[derive(Debug, Clone)]
pub enum Value {
    String(String),
    Integer(i32),
    Float(f32),
    Boolean(bool),
    Player(String),
    Vec3(Vec3),
    ColumnPos(ColumnPos),
    Container(ContainerType),
    Pattern(WorldEditPattern),
    Mask(WorldEditPattern),
    Direction(Direction),
    DirectionExt(DirectionExt),
    BlockPos(BlockPos),
    GreedyString(String),
    Flags(FxHashSet<String>),
}

impl Value {
    fn type_error(&self, expected: &str) -> InternalError {
        InternalError::WrongArgumentType {
            name: format!("{:?}", self),
            expected: expected.to_string(),
        }
    }

    pub(super) fn as_string(&self) -> CommandResult<&String> {
        match self {
            Value::String(s) => Ok(s),
            _ => Err(self.type_error("String").into()),
        }
    }

    pub(super) fn as_integer(&self) -> CommandResult<i32> {
        match self {
            Value::Integer(i) => Ok(*i),
            _ => Err(self.type_error("Integer").into()),
        }
    }

    pub(super) fn as_float(&self) -> CommandResult<f32> {
        match self {
            Value::Float(f) => Ok(*f),
            _ => Err(self.type_error("Float").into()),
        }
    }

    pub(super) fn as_boolean(&self) -> CommandResult<bool> {
        match self {
            Value::Boolean(b) => Ok(*b),
            _ => Err(self.type_error("Boolean").into()),
        }
    }

    pub(super) fn as_player(&self) -> CommandResult<&String> {
        match self {
            Value::Player(p) => Ok(p),
            _ => Err(self.type_error("Player").into()),
        }
    }

    pub(super) fn as_vec3(&self) -> CommandResult<Vec3> {
        match self {
            Value::Vec3(v) => Ok(*v),
            _ => Err(self.type_error("Vec3").into()),
        }
    }

    pub(super) fn as_column_pos(&self) -> CommandResult<ColumnPos> {
        match self {
            Value::ColumnPos(p) => Ok(*p),
            _ => Err(self.type_error("ColumnPos").into()),
        }
    }

    pub(super) fn as_container(&self) -> CommandResult<ContainerType> {
        match self {
            Value::Container(c) => Ok(*c),
            _ => Err(self.type_error("Container").into()),
        }
    }

    pub(super) fn as_pattern(&self) -> CommandResult<&WorldEditPattern> {
        match self {
            Value::Pattern(p) => Ok(p),
            _ => Err(self.type_error("Pattern").into()),
        }
    }

    pub(super) fn as_mask(&self) -> CommandResult<&WorldEditPattern> {
        match self {
            Value::Mask(m) => Ok(m),
            _ => Err(self.type_error("Mask").into()),
        }
    }

    pub(super) fn as_direction(&self) -> CommandResult<Direction> {
        match self {
            Value::Direction(d) => Ok(*d),
            _ => Err(self.type_error("Direction").into()),
        }
    }

    pub(super) fn as_direction_ext(&self) -> CommandResult<DirectionExt> {
        match self {
            Value::DirectionExt(v) => Ok(*v),
            _ => Err(self.type_error("DirectionExt").into()),
        }
    }

    pub(super) fn as_block_pos(&self) -> CommandResult<BlockPos> {
        match self {
            Value::BlockPos(p) => Ok(*p),
            _ => Err(self.type_error("BlockPos").into()),
        }
    }

    pub(super) fn as_greedy(&self) -> CommandResult<&String> {
        match self {
            Value::GreedyString(s) => Ok(s),
            _ => Err(self.type_error("Greedy").into()),
        }
    }

    pub(super) fn as_flags(&self) -> CommandResult<&FxHashSet<String>> {
        match self {
            Value::Flags(f) => Ok(f),
            _ => Err(self.type_error("Flags").into()),
        }
    }
}
