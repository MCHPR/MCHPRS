use super::{BlockDirection, BlockProperty, BlockTransform, FlipDirection};
use std::str::FromStr;

#[derive(Copy, Clone, Debug, PartialEq, Eq, BlockProperty, BlockTransform)]
pub struct RedstoneRepeater {
    pub delay: u8,
    pub facing: BlockDirection,
    pub locked: bool,
    pub powered: bool,
}

impl Default for RedstoneRepeater {
    fn default() -> Self {
        RedstoneRepeater {
            delay: 1,
            facing: Default::default(),
            locked: false,
            powered: false,
        }
    }
}

impl RedstoneRepeater {
    pub(super) fn new(
        delay: u8,
        facing: BlockDirection,
        locked: bool,
        powered: bool,
    ) -> RedstoneRepeater {
        RedstoneRepeater {
            delay,
            facing,
            locked,
            powered,
        }
    }
}

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
pub enum ComparatorMode {
    #[default]
    Compare,
    Subtract,
}

impl ComparatorMode {
    pub(super) fn from_id(id: u32) -> ComparatorMode {
        match id {
            0 => ComparatorMode::Compare,
            1 => ComparatorMode::Subtract,
            _ => panic!("Invalid ComparatorMode"),
        }
    }

    pub(super) fn get_id(self) -> u32 {
        match self {
            ComparatorMode::Compare => 0,
            ComparatorMode::Subtract => 1,
        }
    }

    pub fn toggle(self) -> ComparatorMode {
        match self {
            ComparatorMode::Subtract => ComparatorMode::Compare,
            ComparatorMode::Compare => ComparatorMode::Subtract,
        }
    }
}

impl FromStr for ComparatorMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "subtract" => ComparatorMode::Subtract,
            "compare" => ComparatorMode::Compare,
            _ => return Err(()),
        })
    }
}

impl ToString for ComparatorMode {
    fn to_string(&self) -> String {
        match self {
            ComparatorMode::Subtract => "subtract".to_owned(),
            ComparatorMode::Compare => "compare".to_owned(),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, BlockProperty, BlockTransform)]
pub struct RedstoneComparator {
    pub facing: BlockDirection,
    pub mode: ComparatorMode,
    pub powered: bool,
}

impl RedstoneComparator {
    pub fn new(facing: BlockDirection, mode: ComparatorMode, powered: bool) -> RedstoneComparator {
        RedstoneComparator {
            facing,
            mode,
            powered,
        }
    }
}

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
pub enum LeverFace {
    Floor,
    #[default]
    Wall,
    Ceiling,
}

impl LeverFace {
    pub(super) fn from_id(id: u32) -> LeverFace {
        match id {
            0 => LeverFace::Floor,
            1 => LeverFace::Wall,
            2 => LeverFace::Ceiling,
            _ => panic!("Invalid LeverFace"),
        }
    }

    pub(super) fn get_id(self) -> u32 {
        match self {
            LeverFace::Floor => 0,
            LeverFace::Wall => 1,
            LeverFace::Ceiling => 2,
        }
    }
}

impl FromStr for LeverFace {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "floor" => LeverFace::Floor,
            "ceiling" => LeverFace::Ceiling,
            "wall" => LeverFace::Wall,
            _ => return Err(()),
        })
    }
}

impl ToString for LeverFace {
    fn to_string(&self) -> String {
        match self {
            LeverFace::Floor => "floor".to_owned(),
            LeverFace::Ceiling => "ceiling".to_owned(),
            LeverFace::Wall => "wall".to_owned(),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, BlockProperty, BlockTransform)]
pub struct Lever {
    pub face: LeverFace,
    pub facing: BlockDirection,
    pub powered: bool,
}

impl Lever {
    pub fn new(face: LeverFace, facing: BlockDirection, powered: bool) -> Lever {
        Lever {
            face,
            facing,
            powered,
        }
    }
}

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
pub enum ButtonFace {
    Floor,
    #[default]
    Wall,
    Ceiling,
}

impl ButtonFace {
    pub(super) fn from_id(id: u32) -> ButtonFace {
        match id {
            0 => ButtonFace::Floor,
            1 => ButtonFace::Wall,
            2 => ButtonFace::Ceiling,
            _ => panic!("Invalid ButtonFace"),
        }
    }

    pub(super) fn get_id(self) -> u32 {
        match self {
            ButtonFace::Floor => 0,
            ButtonFace::Wall => 1,
            ButtonFace::Ceiling => 2,
        }
    }
}

impl FromStr for ButtonFace {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "floor" => ButtonFace::Floor,
            "ceiling" => ButtonFace::Ceiling,
            "wall" => ButtonFace::Wall,
            _ => return Err(()),
        })
    }
}

impl ToString for ButtonFace {
    fn to_string(&self) -> String {
        match self {
            ButtonFace::Floor => "floor".to_owned(),
            ButtonFace::Ceiling => "ceiling".to_owned(),
            ButtonFace::Wall => "wall".to_owned(),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, BlockProperty, BlockTransform)]
pub struct StoneButton {
    pub face: ButtonFace,
    pub facing: BlockDirection,
    pub powered: bool,
}

impl StoneButton {
    pub fn new(face: ButtonFace, facing: BlockDirection, powered: bool) -> StoneButton {
        StoneButton {
            face,
            facing,
            powered,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum RedstoneWireSide {
    Up,
    Side,
    #[default]
    None,
}

impl RedstoneWireSide {
    pub fn is_none(self) -> bool {
        matches!(self, RedstoneWireSide::None)
    }
}

impl FromStr for RedstoneWireSide {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "up" => RedstoneWireSide::Up,
            "side" => RedstoneWireSide::Side,
            "none" => RedstoneWireSide::None,
            _ => return Err(()),
        })
    }
}

impl ToString for RedstoneWireSide {
    fn to_string(&self) -> String {
        match self {
            RedstoneWireSide::Up => "up".to_owned(),
            RedstoneWireSide::Side => "side".to_owned(),
            RedstoneWireSide::None => "none".to_owned(),
        }
    }
}

impl RedstoneWireSide {
    pub fn from_id(id: u32) -> RedstoneWireSide {
        match id {
            0 => RedstoneWireSide::Up,
            1 => RedstoneWireSide::Side,
            2 => RedstoneWireSide::None,
            _ => panic!("Invalid RedstoneWireSide"),
        }
    }

    pub fn get_id(self) -> u32 {
        match self {
            RedstoneWireSide::Up => 0,
            RedstoneWireSide::Side => 1,
            RedstoneWireSide::None => 2,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, BlockProperty)]
pub struct RedstoneWire {
    pub north: RedstoneWireSide,
    pub south: RedstoneWireSide,
    pub east: RedstoneWireSide,
    pub west: RedstoneWireSide,
    pub power: u8,
}

impl RedstoneWire {
    pub fn new(
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

impl BlockTransform for RedstoneWire {
    fn rotate90(&mut self) {
        *self = RedstoneWire {
            north: self.west,
            east: self.north,
            south: self.east,
            west: self.south,
            ..*self
        }
    }

    fn flip(&mut self, dir: FlipDirection) {
        *self = match dir {
            FlipDirection::FlipX => RedstoneWire {
                east: self.west,
                west: self.east,
                ..*self
            },
            FlipDirection::FlipZ => RedstoneWire {
                north: self.south,
                south: self.north,
                ..*self
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrapdoorHalf {
    Top,
    Bottom,
}

impl TrapdoorHalf {
    pub fn get_id(self) -> u32 {
        self as u32
    }

    pub fn from_id(id: u32) -> TrapdoorHalf {
        use TrapdoorHalf::*;
        match id {
            0 => Top,
            1 => Bottom,
            _ => unreachable!(),
        }
    }
}

impl ToString for TrapdoorHalf {
    fn to_string(&self) -> String {
        match self {
            TrapdoorHalf::Top => "top".to_owned(),
            TrapdoorHalf::Bottom => "bottom".to_owned(),
        }
    }
}

impl FromStr for TrapdoorHalf {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "top" => TrapdoorHalf::Top,
            "bottom" => TrapdoorHalf::Bottom,
            _ => return Err(()),
        })
    }
}
