use mchprs_proc_macros::protocol_id;

use super::{Block, BlockDirection, BlockProperty, BlockTransform, FlipDirection};

#[derive(Copy, Clone, Debug, PartialEq, Eq, BlockProperty, BlockTransform)]
pub struct Repeater {
    pub delay: u8,
    pub facing: BlockDirection,
    pub locked: bool,
    pub powered: bool,
}

impl Default for Repeater {
    fn default() -> Self {
        Repeater {
            delay: 1,
            facing: Default::default(),
            locked: false,
            powered: false,
        }
    }
}

impl Repeater {
    pub fn new(delay: u8, facing: BlockDirection, locked: bool, powered: bool) -> Repeater {
        Repeater {
            delay,
            facing,
            locked,
            powered,
        }
    }
}

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, Hash)]
pub enum ComparatorMode {
    #[default]
    Compare,
    Subtract,
}

impl ComparatorMode {
    pub fn toggle(self) -> ComparatorMode {
        match self {
            ComparatorMode::Subtract => ComparatorMode::Compare,
            ComparatorMode::Compare => ComparatorMode::Subtract,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, BlockProperty, BlockTransform)]
pub struct Comparator {
    pub facing: BlockDirection,
    pub mode: ComparatorMode,
    pub powered: bool,
}

impl Comparator {
    pub fn new(facing: BlockDirection, mode: ComparatorMode, powered: bool) -> Comparator {
        Comparator {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TrapdoorHalf {
    #[default]
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Instrument {
    Harp,
    Basedrum,
    Snare,
    Hat,
    Bass,
    Flute,
    Bell,
    Guitar,
    Chime,
    Xylophone,
    IronXylophone,
    CowBell,
    Didgeridoo,
    Bit,
    Banjo,
    Pling,
    Zombie,
    Skeleton,
    Creeper,
    Dragon,
    WitherSkeleton,
    Piglin,
    CustomHead,
}

impl Instrument {
    pub fn from_block_below(block: Block) -> Instrument {
        match block {
            // All stone materials
            _ if block.is_stone() => Instrument::Basedrum,
            // All sand/aggregate materials: ConcretePowder
            Block::Sand => Instrument::Snare,
            // All glass materials: GlassPane
            _ if block.is_glass() => Instrument::Hat,
            // All wood materials: Log, Plank
            _ if block.is_wood() => Instrument::Bass,
            Block::Clay => Instrument::Flute,
            Block::GoldBlock => Instrument::Bell,
            _ if block.is_wool() => Instrument::Guitar,
            Block::PackedIce => Instrument::Chime,
            Block::BoneBlock { .. } => Instrument::Xylophone,
            Block::IronBlock => Instrument::IronXylophone,
            Block::SoulSand => Instrument::CowBell,
            Block::Pumpkin => Instrument::Didgeridoo,
            Block::EmeraldBlock => Instrument::Bit,
            Block::HayBlock { .. } => Instrument::Banjo,
            Block::Glowstone => Instrument::Pling,
            _ => Instrument::Harp,
        }
    }

    pub fn to_sound_id(&self) -> i32 {
        macro_rules! sound_id {
            ($name:literal) => {
                protocol_id!("minecraft:sound_event", $name)
            };
        }
        match self {
            Instrument::Harp => sound_id!("minecraft:block.note_block.harp"),
            Instrument::Basedrum => sound_id!("minecraft:block.note_block.basedrum"),
            Instrument::Snare => sound_id!("minecraft:block.note_block.snare"),
            Instrument::Hat => sound_id!("minecraft:block.note_block.hat"),
            Instrument::Bass => sound_id!("minecraft:block.note_block.bass"),
            Instrument::Flute => sound_id!("minecraft:block.note_block.flute"),
            Instrument::Bell => sound_id!("minecraft:block.note_block.bell"),
            Instrument::Guitar => sound_id!("minecraft:block.note_block.guitar"),
            Instrument::Chime => sound_id!("minecraft:block.note_block.chime"),
            Instrument::Xylophone => sound_id!("minecraft:block.note_block.xylophone"),
            Instrument::IronXylophone => sound_id!("minecraft:block.note_block.iron_xylophone"),
            Instrument::CowBell => sound_id!("minecraft:block.note_block.cow_bell"),
            Instrument::Didgeridoo => sound_id!("minecraft:block.note_block.didgeridoo"),
            Instrument::Bit => sound_id!("minecraft:block.note_block.bit"),
            Instrument::Banjo => sound_id!("minecraft:block.note_block.banjo"),
            Instrument::Pling => sound_id!("minecraft:block.note_block.pling"),
            Instrument::Zombie => sound_id!("minecraft:block.note_block.imitate.zombie"),
            Instrument::Skeleton => sound_id!("minecraft:block.note_block.imitate.skeleton"),
            Instrument::Creeper => sound_id!("minecraft:block.note_block.imitate.creeper"),
            Instrument::Dragon => sound_id!("minecraft:block.note_block.imitate.ender_dragon"),
            Instrument::WitherSkeleton => {
                sound_id!("minecraft:block.note_block.imitate.wither_skeleton")
            }
            Instrument::Piglin => sound_id!("minecraft:block.note_block.imitate.piglin"),
            _ => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HopperFacing {
    #[default]
    Down,
    North,
    South,
    West,
    East,
}

impl BlockTransform for HopperFacing {
    fn flip(&mut self, dir: FlipDirection) {
        match dir {
            FlipDirection::FlipX => match self {
                Self::East => *self = Self::West,
                Self::West => *self = Self::East,
                _ => {}
            },
            FlipDirection::FlipZ => match self {
                Self::North => *self = Self::South,
                Self::South => *self = Self::North,
                _ => {}
            },
        }
    }

    fn rotate90(&mut self) {
        *self = match *self {
            Self::North => Self::East,
            Self::East => Self::South,
            Self::South => Self::West,
            Self::West => Self::North,
            x => x,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SlabType {
    Top,
    #[default]
    Bottom,
    Double,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BlockAxis {
    X,
    #[default]
    Y,
    Z,
}

impl BlockTransform for BlockAxis {
    fn rotate90(&mut self) {
        *self = match *self {
            Self::X => Self::Z,
            Self::Z => Self::X,
            x => x,
        }
    }
    fn flip(&mut self, _dir: FlipDirection) {}
}
