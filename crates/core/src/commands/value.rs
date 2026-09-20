use super::error::RuntimeError;
use crate::worldedit::{mask::WorldEditMask, pattern::WorldEditPattern};
use mchprs_blocks::{block_entities::ContainerType, BlockFacing, BlockPos};
use rustc_hash::FxHashSet;
use std::{array, f64::consts::FRAC_PI_2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coordinate<T> {
    Absolute(T),
    Relative(T),
}

impl Coordinate<i32> {
    pub fn resolve(&self, reference: i32) -> Result<i32, RuntimeError> {
        match self {
            Self::Absolute(value) => Some(*value),
            Self::Relative(offset) => reference.checked_add(*offset),
        }
        .ok_or(RuntimeError::CoordinateOutOfRange)
    }
}

impl Coordinate<f64> {
    pub fn resolve(&self, reference: f64) -> f64 {
        match self {
            Coordinate::Absolute(value) => *value,
            Coordinate::Relative(offset) => reference + *offset,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
            Self::Up => BlockFacing::Up,
            Self::Down => BlockFacing::Down,
            Self::North => BlockFacing::North,
            Self::South => BlockFacing::South,
            Self::East => BlockFacing::East,
            Self::West => BlockFacing::West,
            Self::Me => player_facing,
            Self::Left => player_facing.rotate_ccw(),
            Self::Right => player_facing.rotate(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectionWithDiagonals {
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

impl DirectionWithDiagonals {
    pub fn resolve(&self, player_facing: BlockFacing, player_pitch: f32) -> BlockPos {
        match self {
            Self::Up => BlockPos::new(0, 1, 0),
            Self::Down => BlockPos::new(0, -1, 0),
            Self::North => BlockPos::new(0, 0, -1),
            Self::South => BlockPos::new(0, 0, 1),
            Self::East => BlockPos::new(1, 0, 0),
            Self::West => BlockPos::new(-1, 0, 0),
            Self::NorthUp => BlockPos::new(0, 1, -1),
            Self::NorthDown => BlockPos::new(0, -1, -1),
            Self::SouthUp => BlockPos::new(0, 1, 1),
            Self::SouthDown => BlockPos::new(0, -1, 1),
            Self::EastUp => BlockPos::new(1, 1, 0),
            Self::EastDown => BlockPos::new(1, -1, 0),
            Self::WestUp => BlockPos::new(-1, 1, 0),
            Self::WestDown => BlockPos::new(-1, -1, 0),
            Self::Me => {
                let mut offset = player_facing.offset_pos(BlockPos::zero(), 1);
                if !matches!(player_facing, BlockFacing::Down | BlockFacing::Up) {
                    if player_pitch > 22.5 {
                        offset.y -= 1;
                    } else if player_pitch < -22.5 {
                        offset.y += 1;
                    }
                }
                offset
            }
            Self::Left => player_facing.rotate_ccw().offset_pos(BlockPos::zero(), 1),
            Self::Right => player_facing.rotate().offset_pos(BlockPos::zero(), 1),
            Self::LeftUp => player_facing
                .rotate_ccw()
                .offset_pos(BlockPos::new(0, 1, 0), 1),
            Self::LeftDown => player_facing
                .rotate_ccw()
                .offset_pos(BlockPos::new(0, -1, 0), 1),
            Self::RightUp => player_facing.rotate().offset_pos(BlockPos::new(0, 1, 0), 1),
            Self::RightDown => player_facing
                .rotate()
                .offset_pos(BlockPos::new(0, -1, 0), 1),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PositionCoordinates {
    WorldAxes {
        x: Coordinate<f64>,
        y: Coordinate<f64>,
        z: Coordinate<f64>,
    },
    ViewAxes {
        left: f64,
        up: f64,
        forward: f64,
    },
}

impl PositionCoordinates {
    pub fn resolve(
        &self,
        reference: (f64, f64, f64),
        yaw: f32,
        pitch: f32,
    ) -> Result<(f64, f64, f64), RuntimeError> {
        let [x, y, z] = match self {
            Self::WorldAxes { x, y, z } => [
                x.resolve(reference.0),
                y.resolve(reference.1),
                z.resolve(reference.2),
            ],
            Self::ViewAxes { left, up, forward } => {
                let yaw = (yaw as f64 + 90.0).to_radians();
                let pitch = (-pitch as f64).to_radians();
                let forward_axis = [
                    yaw.cos() * pitch.cos(),
                    pitch.sin(),
                    yaw.sin() * pitch.cos(),
                ];
                let up_pitch = pitch + FRAC_PI_2;
                let up_axis = [
                    yaw.cos() * up_pitch.cos(),
                    up_pitch.sin(),
                    yaw.sin() * up_pitch.cos(),
                ];
                let left_axis = [
                    forward_axis[2] * up_axis[1] - forward_axis[1] * up_axis[2],
                    forward_axis[0] * up_axis[2] - forward_axis[2] * up_axis[0],
                    forward_axis[1] * up_axis[0] - forward_axis[0] * up_axis[1],
                ];
                let origin = [reference.0, reference.1, reference.2];
                array::from_fn(|i| {
                    origin[i] + left * left_axis[i] + up * up_axis[i] + forward * forward_axis[i]
                })
            }
        };
        if [x, y, z].into_iter().all(f64::is_finite) {
            Ok((x, y, z))
        } else {
            Err(RuntimeError::CoordinateOutOfRange)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlotCoordinates {
    pub x: Coordinate<i32>,
    pub z: Coordinate<i32>,
}

impl PlotCoordinates {
    pub fn resolve(&self, reference: (i32, i32)) -> Result<(i32, i32), RuntimeError> {
        Ok((self.x.resolve(reference.0)?, self.z.resolve(reference.1)?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockCoordinates(pub PositionCoordinates);

impl BlockCoordinates {
    pub fn resolve(
        &self,
        reference: (f64, f64, f64),
        yaw: f32,
        pitch: f32,
    ) -> Result<BlockPos, RuntimeError> {
        let (x, y, z) = self.0.resolve(reference, yaw, pitch)?;
        let coordinates = [x.floor(), y.floor(), z.floor()];
        if coordinates
            .iter()
            .any(|&value| value < i32::MIN as f64 || value > i32::MAX as f64)
        {
            return Err(RuntimeError::CoordinateOutOfRange);
        }
        Ok(BlockPos::new(
            coordinates[0] as i32,
            coordinates[1] as i32,
            coordinates[2] as i32,
        ))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReplacementOperand {
    pub pattern: Option<WorldEditPattern>,
    pub mask: Option<WorldEditMask>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerTarget {
    Name(String),
    Uuid(u128),
    SelfPlayer,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    String(String),
    Player(PlayerTarget),
    Replacement(ReplacementOperand),
    Integer(i32),
    Float(f32),
    Position(PositionCoordinates),
    PlotPos(PlotCoordinates),
    Container(ContainerType),
    Pattern(WorldEditPattern),
    Mask(WorldEditMask),
    Direction(Direction),
    DirectionWithDiagonals(DirectionWithDiagonals),
    BlockPos(BlockCoordinates),
    Flags(FxHashSet<String>),
}

pub trait FromValue<'a>: Sized {
    const TYPE_NAME: &'static str;

    fn from_value(value: &'a Value) -> Option<Self>;
}

macro_rules! from_value {
    ($ty:ty, $name:literal, $pattern:pat => $result:expr) => {
        impl<'a> FromValue<'a> for $ty {
            const TYPE_NAME: &'static str = $name;

            fn from_value(value: &'a Value) -> Option<Self> {
                match value {
                    $pattern => Some($result),
                    _ => None,
                }
            }
        }
    };
}

from_value!(PlayerTarget, "PlayerTarget", Value::Player(p) => p.clone());
from_value!(&'a ReplacementOperand, "ReplacementOperand", Value::Replacement(p) => p);
from_value!(String, "String", Value::String(s) => s.clone());
from_value!(i32, "Integer", Value::Integer(i) => *i);
from_value!(f32, "Float", Value::Float(f) => *f);
from_value!(PositionCoordinates, "PositionCoordinates", Value::Position(v) => *v);
from_value!(PlotCoordinates, "PlotPos", Value::PlotPos(p) => *p);
from_value!(ContainerType, "Container", Value::Container(c) => *c);
from_value!(&'a WorldEditPattern, "Pattern", Value::Pattern(p) => p);
from_value!(&'a WorldEditMask, "Mask", Value::Mask(m) => m);
from_value!(Direction, "Direction", Value::Direction(d) => *d);
from_value!(DirectionWithDiagonals, "DirectionWithDiagonals", Value::DirectionWithDiagonals(d) => *d);
from_value!(BlockCoordinates, "BlockPos", Value::BlockPos(p) => *p);
