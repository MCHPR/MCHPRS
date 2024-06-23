mod props;

use crate::{BlockColorVariant, BlockDirection, BlockFacing, BlockProperty, SignType};
use mchprs_proc_macros::BlockTransform;
pub use props::*;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
pub enum FlipDirection {
    FlipX,
    FlipZ,
}

#[derive(Clone, Copy, Debug)]
pub enum RotateAmt {
    Rotate90,
    Rotate180,
    Rotate270,
}

trait BlockTransform {
    fn rotate(&mut self, amt: crate::blocks::RotateAmt) {
        match amt {
            // ez
            RotateAmt::Rotate90 => self.rotate90(),
            RotateAmt::Rotate180 => {
                self.rotate90();
                self.rotate90();
            }
            RotateAmt::Rotate270 => {
                self.rotate90();
                self.rotate90();
                self.rotate90();
            }
        }
    }
    fn rotate90(&mut self);
    fn flip(&mut self, dir: crate::blocks::FlipDirection);
}

macro_rules! noop_block_transform {
    ($($ty:ty),*$(,)?) => {
        $(
            impl BlockTransform for $ty {
                fn rotate90(&mut self) {}
                fn flip(&mut self, _dir: crate::blocks::FlipDirection) {}
            }
        )*
    };
}

noop_block_transform!(
    u8,
    u32,
    bool,
    BlockColorVariant,
    BlockFacing,
    TrapdoorHalf,
    SignType,
    ButtonFace,
    LeverFace,
    ComparatorMode,
    Instrument,
);

impl BlockTransform for BlockDirection {
    fn flip(&mut self, dir: FlipDirection) {
        match dir {
            FlipDirection::FlipX => match self {
                BlockDirection::East => *self = BlockDirection::West,
                BlockDirection::West => *self = BlockDirection::East,
                _ => {}
            },
            FlipDirection::FlipZ => match self {
                BlockDirection::North => *self = BlockDirection::South,
                BlockDirection::South => *self = BlockDirection::North,
                _ => {}
            },
        }
    }

    fn rotate90(&mut self) {
        *self = match self {
            BlockDirection::North => BlockDirection::East,
            BlockDirection::East => BlockDirection::South,
            BlockDirection::South => BlockDirection::West,
            BlockDirection::West => BlockDirection::North,
        }
    }
}

impl Block {
    pub fn has_block_entity(self) -> bool {
        matches!(
            self,
            Block::RedstoneComparator { .. }
                | Block::Barrel { .. }
                | Block::Furnace { .. }
                | Block::Hopper { .. }
                | Block::Sign { .. }
                | Block::WallSign { .. }
        )
    }

    pub fn can_place_block_in(self) -> bool {
        matches!(self.get_id(),
            0               // Air
            | 12958..=12959 // Void and Cave air
            | 80..=95       // Water
            | 96..=111      // Lava
            | 2005          // Short Grass
            | 2006          // Fern
            | 2007          // Dead bush
            | 2008          // Seagrass
            | 2009..=2010   // Tall Seagrass
            | 10755..=10756 // Tall Grass
            | 10757..=10758 // Large Fern
        )
    }
}

#[test]
fn repeater_id_test() {
    let original = Block::RedstoneRepeater {
        repeater: RedstoneRepeater::new(3, BlockDirection::West, true, false),
    };
    let id = original.get_id();
    assert_eq!(id, 4141);
    let new = Block::from_id(id);
    assert_eq!(new, original);
}

#[test]
fn comparator_id_test() {
    let original = Block::RedstoneComparator {
        comparator: RedstoneComparator::new(BlockDirection::West, ComparatorMode::Subtract, false),
    };
    let id = original.get_id();
    assert_eq!(id, 6895);
    let new = Block::from_id(id);
    assert_eq!(new, original);
}

macro_rules! blocks {
    (
        $(
            $name:ident {
                props: {
                    $(
                        $prop_name:ident : $prop_type:ident
                    ),*
                },
                get_id: $get_id:expr,
                $( from_id_offset: $get_id_offset:literal, )?
                from_id($id_name:ident): $from_id_pat:pat => {
                    $(
                        $from_id_pkey:ident: $from_id_pval:expr
                    ),*
                },
                from_names($name_name:ident): {
                    $(
                        $from_name_pat:pat => {
                            $(
                                $from_name_pkey:ident: $from_name_pval:expr
                            ),*
                        }
                    ),*
                },
                get_name: $get_name:expr,
                $( solid: $solid:literal, )?
                $( transparent: $transparent:literal, )?
                $( cube: $cube:literal, )?
            }
        ),*
    ) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Block {
            $(
                $name {
                    $(
                        $prop_name: $prop_type,
                    )*
                }
            ),*
        }

        #[allow(clippy::redundant_field_names)]
        impl Block {
            pub fn is_solid(self) -> bool {
                match self {
                    $(
                        $( Block::$name { .. } => $solid, )?
                    )*
                    _ => false
                }
            }

            pub fn is_transparent(self) -> bool {
                match self {
                    $(
                        $( Block::$name { .. } => $transparent, )?
                    )*
                    _ => false
                }
            }

            pub fn is_cube(self) -> bool {
                match self {
                    $(
                        $( Block::$name { .. } => $cube, )?
                    )*
                    _ => false
                }
            }

            pub fn get_id(self) -> u32 {
                match self {
                    $(
                        Block::$name {
                            $(
                                $prop_name,
                            )*
                        } => $get_id,
                    )*
                }
            }

            pub fn from_id(mut id: u32) -> Block {
                match id {
                    $(
                        $from_id_pat => {
                            $( id -= $get_id_offset; )?
                            let $id_name = id;
                            Block::$name {
                                $(
                                    $from_id_pkey: $from_id_pval
                                ),*
                            }
                        },
                    )*
                }
            }

            pub fn from_name(name: &str) -> Option<Block> {
                match name {
                    $(
                        $(
                            $from_name_pat => {
                                let $name_name = name;
                                Some(Block::$name {
                                    $(
                                        $from_name_pkey: $from_name_pval
                                    ),*
                                })
                            },
                        )*
                    )*
                    _ => None,
                }
            }

            // Not all props will be part of the name
            #[allow(unused_variables)]
            pub fn get_name(self) -> &'static str {
                match self {
                    $(
                        Block::$name {
                            $(
                                $prop_name,
                            )*
                        } => $get_name,
                    )*
                }
            }

            pub fn set_properties(&mut self, props: HashMap<&str, &str>) {
                match self {
                    $(
                        Block::$name {
                            $(
                                $prop_name,
                            )*
                        } => {
                            $(
                                <$prop_type as BlockProperty>::decode($prop_name, &props, stringify!($prop_name));
                            )*
                        },
                    )*
                }
            }

            pub fn properties(&self) -> HashMap<&'static str, String> {
                let mut props = HashMap::new();
                match self {
                    $(
                        Block::$name {
                            $(
                                $prop_name,
                            )*
                        } => {
                            $(
                                <$prop_type as BlockProperty>::encode(*$prop_name, &mut props, stringify!($prop_name));
                            )*
                        },
                    )*
                }
                props
            }

            pub fn rotate(&mut self, amt: RotateAmt) {
                match self {
                    $(
                        Block::$name {
                            $(
                                $prop_name,
                            )*
                        } => {
                            $(
                                <$prop_type as BlockTransform>::rotate($prop_name, amt);
                            )*
                        },
                    )*
                }
            }

            pub fn flip(&mut self, dir: FlipDirection) {
                match self {
                    $(
                        Block::$name {
                            $(
                                $prop_name,
                            )*
                        } => {
                            $(
                                <$prop_type as BlockTransform>::flip($prop_name, dir);
                            )*
                        },
                    )*
                }
            }
        }
    }
}

blocks! {
    Air {
        props: {},
        get_id: 0,
        from_id(_id): 0 => {},
        from_names(_name): {
            "air" => {}
        },
        get_name: "air",
    },
    Stone {
        props: {},
        get_id: 1,
        from_id(_id): 1 => {},
        from_names(_name): {
            "stone" => {}
        },
        get_name: "stone",
        solid: true,
        cube: true,
    },
    Glass {
        props: {},
        get_id: 519,
        from_id(_id): 519 => {},
        from_names(_name): {
            "glass" => {}
        },
        get_name: "glass",
        transparent: true,
        cube: true,
    },
    Glowstone {
        props: {},
        get_id: 5863,
        from_id(_id): 5863 => {},
        from_names(_name): {
            "glowstone" => {}
        },
        get_name: "glowstone",
        transparent: true,
        cube: true,
    },
    RedstoneWire {
        props: {
            wire: RedstoneWire
        },
        get_id: {
            wire.east.get_id() * 432
                + wire.north.get_id() * 144
                + wire.power as u32 * 9
                + wire.south.get_id() * 3
                + wire.west.get_id()
                + 2978
        },
        from_id_offset: 2978,
        from_id(id): 2978..=4273 => {
            wire: RedstoneWire::new(
                RedstoneWireSide::from_id(id % 432 / 144),
                RedstoneWireSide::from_id(id % 9 / 3),
                RedstoneWireSide::from_id(id / 432),
                RedstoneWireSide::from_id(id % 3),
                (id % 144 / 9) as u8,
            )
        },
        from_names(_name): {
            "redstone_wire" => {
                wire: Default::default()
            }
        },
        get_name: "redstone_wire",
    },
    WallSign {
        props: {
            sign_type: SignType,
            facing: BlockDirection
        },
        get_id: (sign_type.0 << 3) + (facing.get_id() << 1) + match sign_type.0 {
            0..=5 => 4763,
            6..=7 => 19341 - (6 << 3),
            _ => unreachable!(),
        },
        from_id_offset: 0,
        from_id(id): 4763..=4817 | 19341..=19355 => {
            sign_type: SignType(match id {
                4763..=4817 => (id - 4763) >> 3,
                19341..=19355 => ((id - 19341) >> 3) + 6,
                _ => unreachable!(),
            }),
            facing: BlockDirection::from_id((match id {
                4763..=4817 => id - 4763,
                19341..=19355=> id - 19341,
                _ => unreachable!(),
            } & 0b110) >> 1)
        },
        from_names(_name): {
            "oak_wall_sign" => {
                sign_type: SignType(0),
                facing: Default::default()
            },
            "spruce_wall_sign" => {
                sign_type: SignType(1),
                facing: Default::default()
            },
            "birch_wall_sign" => {
                sign_type: SignType(2),
                facing: Default::default()
            },
            "acacia_wall_sign" => {
                sign_type: SignType(3),
                facing: Default::default()
            },
            "jungle_wall_sign" => {
                sign_type: SignType(4),
                facing: Default::default()
            },
            "dark_oak_wall_sign" => {
                sign_type: SignType(5),
                facing: Default::default()
            },
            "crimson_wall_sign" => {
                sign_type: SignType(6),
                facing: Default::default()
            },
            "warped_wall_sign" => {
                sign_type: SignType(7),
                facing: Default::default()
            }
        },
        get_name: match sign_type.0 {
            0 => "oak_wall_sign",
            1 => "spruce_wall_sign",
            2 => "birch_wall_sign",
            3 => "acacia_wall_sign",
            4 => "jungle_wall_sign",
            5 => "dark_oak_wall_sign",
            6 => "crimson_wall_sign",
            7 => "warped_wall_sign",
            _ => "invalid_wall_sign"
        },
    },
    Lever {
        props: {
            lever: Lever
        },
        get_id: {
            (lever.face.get_id() << 3)
                + (lever.facing.get_id() << 1)
                + !lever.powered as u32
                + 5626
        },
        from_id_offset: 5626,
        from_id(id): 5626..=5649 => {
            lever: Lever::new(
                LeverFace::from_id(id >> 3),
                BlockDirection::from_id((id >> 1) & 0b11),
                (id & 1) == 0
            )
        },
        from_names(_name): {
            "lever" => {
                lever: Default::default()
            }
        },
        get_name: "lever",
    },
    StoneButton {
        props: {
            button: StoneButton
        },
        get_id: {
            (button.face.get_id() << 3)
                + (button.facing.get_id() << 1)
                + !button.powered as u32
                + 5748
        },
        from_id_offset: 5748,
        from_id(id): 5748..=5771=> {
            button: StoneButton::new(ButtonFace::from_id(id >> 3), BlockDirection::from_id((id >> 1) & 0b11), (id & 1) == 0)
        },
        from_names(_name): {
            "stone_button" => {
                button: Default::default()
            }
        },
        get_name: "stone_button",
    },
    Sign {
        props: {
            sign_type: SignType,
            rotation: u32
        },
        get_id: (sign_type.0 << 5) + (rotation << 1) + match sign_type.0 {
            0..=5 => 4303,
            6..=7 => 19277 - (6 << 5),
            _ => unreachable!(),
        },
        from_id_offset: 0,
        from_id(id): 4303..=4525 | 19277..=19339 => {
            sign_type: SignType(match id {
                4303..=4525 => (id - 4303) >> 5,
                19277..=19339 => ((id - 19277) >> 5) + 6,
                _ => unreachable!(),
            }),
            rotation: (match id {
                4303..=4525=> id - 4303,
                19277..=19339 => id - 19277,
                _ => unreachable!(),
            } & 0b11110) >> 1
        },
        from_names(_name): {
            "oak_sign" => {
                sign_type: SignType(0),
                rotation: 0
            },
            "spruce_sign" => {
                sign_type: SignType(1),
                rotation: 0
            },
            "birch_sign" => {
                sign_type: SignType(2),
                rotation: 0
            },
            "acacia_sign" => {
                sign_type: SignType(3),
                rotation: 0
            },
            "jungle_sign" => {
                sign_type: SignType(4),
                rotation: 0
            },
            "dark_oak_sign" => {
                sign_type: SignType(5),
                rotation: 0
            },
            "crimson_sign" => {
                sign_type: SignType(6),
                rotation: 0
            },
            "warped_sign" => {
                sign_type: SignType(7),
                rotation: 0
            }
        },
        get_name: match sign_type.0 {
            0 => "oak_sign",
            1 => "spruce_sign",
            2 => "birch_sign",
            3 => "acacia_sign",
            4 => "jungle_sign",
            5 => "dark_oak_sign",
            6 => "crimson_sign",
            7 => "warped_sign",
            _ => "invalid_sign"
        },
    },
    RedstoneTorch {
        props: {
            lit: bool
        },
        get_id: if lit {
            5738
        } else {
            5739
        },
        from_id_offset: 5738,
        from_id(id): 5738..=5739 => {
            lit: id == 0
        },
        from_names(_name): {
            "redstone_torch" => {
                lit: true
            }
        },
        get_name: "redstone_torch",
    },
    RedstoneWallTorch {
        props: {
            lit: bool,
            facing: BlockDirection
        },
        get_id: (facing.get_id() << 1) + (!lit as u32) + 5740,
        from_id_offset: 5740,
        from_id(id): 5740..=5747 => {
            lit: (id & 1) == 0,
            facing: BlockDirection::from_id(id >> 1)
        },
        from_names(_name): {
            "redstone_wall_torch" => {
                lit: true,
                facing: Default::default()
            }
        },
        get_name: "redstone_wall_torch",
    },
    RedstoneRepeater {
        props: {
            repeater: RedstoneRepeater
        },
        get_id: {
            (repeater.delay as u32 - 1) * 16
                + repeater.facing.get_id() * 4
                + !repeater.locked as u32 * 2
                + !repeater.powered as u32
                + 5881
        },
        from_id_offset: 5881,
        from_id(id): 5881..=5944 => {
            repeater: RedstoneRepeater::new(
                (id >> 4) as u8 + 1,
                BlockDirection::from_id((id >> 2) & 3),
                ((id >> 1) & 1) == 0,
                (id & 1) == 0
            )
        },
        from_names(_name): {
            "repeater" => {
                repeater: Default::default()
            }
        },
        get_name: "repeater",
    },
    RedstoneLamp {
        props: {
            lit: bool
        },
        get_id: if lit {
            7417
        } else {
            7418
        },
        from_id_offset: 7417,
        from_id(id): 7417..=7418 => {
            lit: id == 0
        },
        from_names(_name): {
            "redstone_lamp" => {
                lit: false
            }
        },
        get_name: "redstone_lamp",
        solid: true,
        cube: true,
    },
    TripwireHook {
        props: {
            direction: BlockDirection
        },
        get_id: match direction {
            BlockDirection::North => 7530,
            BlockDirection::South => 7532,
            BlockDirection::West => 7534,
            BlockDirection::East => 7536,
        },
        from_id_offset: 7530,
        from_id(id): 7530..=7536 => {
            direction: BlockDirection::from_id(id / 2)
        },
        from_names(_name): {
            "tripwire_hook" => {
                direction: Default::default()
            }
        },
        get_name: "tripwire_hook",
    },
    RedstoneComparator {
        props: {
            comparator: RedstoneComparator
        },
        get_id: {
            comparator.facing.get_id() * 4
                + comparator.mode.get_id() * 2
                + !comparator.powered as u32
                + 9175
        },
        from_id_offset: 9175,
        from_id(id): 9175..=9190 => {
            comparator: RedstoneComparator::new(
                BlockDirection::from_id(id >> 2),
                ComparatorMode::from_id((id >> 1) & 1),
                (id & 1) == 0
            )
        },
        from_names(_name): {
            "comparator" => {
                comparator: Default::default()
            }
        },
        get_name: "comparator",
    },
    RedstoneBlock {
        props: {},
        get_id: 9223,
        from_id(_id): 9223 => {},
        from_names(_name): {
            "redstone_block" => {}
        },
        get_name: "redstone_block",
        transparent: true,
        cube: true,
    },
    Observer {
        props: {
            facing: BlockFacing
        },
        get_id: (facing.get_id() << 1) + 12551,
        from_id_offset: 12551,
        from_id(id): 12551..=12561 => {
            facing: BlockFacing::from_id(id >> 1)
        },
        from_names(_name): {
            "observer" => {
                facing: Default::default()
            }
        },
        get_name: "observer",
        solid: true,
        cube: true,
    },
    SeaPickle {
        props: {
            pickles: u8
        },
        get_id: ((pickles - 1) << 1) as u32 + 12934,
        from_id_offset: 12934,
        from_id(id): 12934..=12940 => {
            pickles: (id >> 1) as u8 + 1
        },
        from_names(_name): {
            "sea_pickle" => {
                pickles: 1
            }
        },
        get_name: "sea_pickle",
    },
    Target {
        props: {},
        get_id: 19381,
        from_id(_id): 19381 => {},
        from_names(_name): {
            "target" => {}
        },
        get_name: "target",
        solid: true,
        cube: true,
    },
    StonePressurePlate {
        props: {
            powered: bool
        },
        get_id: 5650 + !powered as u32,
        from_id_offset: 5650,
        from_id(id): 5650..=5651 => {
            powered: id == 0
        },
        from_names(_name): {
            "stone_pressure_plate" => {
                powered: false
            }
        },
        get_name: "stone_pressure_plate",
    },
    Cake {
        props: {
            bites: u8
        },
        get_id: 5874 + bites as u32,
        from_id_offset: 5874,
        from_id(id): 5874..=5880 => {
            bites: id as u8
        },
        from_names(_name): {
            "cake" => {
                bites: 0
            }
        },
        get_name: "cake",
    },
    Barrel {
        props: {},
        get_id: 18409,
        from_id(_id): 18409 => {},
        from_names(_name): {
            "barrel" => {}
        },
        get_name: "barrel",
        solid: true,
        cube: true,
    },
    Hopper {
        props: {},
        get_id: 9230,
        from_id(_id): 9230 => {},
        from_names(_name): {
            "hopper" => {}
        },
        get_name: "hopper",
        transparent: true,
        cube: true,
    },
    Sandstone {
        props: {},
        get_id: 535,
        from_id(_id): 535 => {},
        from_names(_name): {
            "sandstone" => {}
        },
        get_name: "sandstone",
        solid: true,
        cube: true,
    },
    CoalBlock {
        props: {},
        get_id: 10745,
        from_id(_id): 10745 => {},
        from_names(_name): {
            "coal_block" => {}
        },
        get_name: "coal_block",
        solid: true,
        cube: true,
    },
    Furnace {
        props: {},
        get_id: 4295,
        from_id(_id): 4295 => {},
        from_names(_name): {
            "furnace" => {}
        },
        get_name: "furnace",
        solid: true,
        cube: true,
    },
    Quartz {
        props: {},
        get_id: 9235,
        from_id(_id): 9235 => {},
        from_names(_name): {
            "quartz_block" => {}
        },
        get_name: "quartz_block",
        solid: true,
        cube: true,
    },
    SmoothQuartz {
        props: {},
        get_id: 11308,
        from_id(_id): 11308 => {},
        from_names(_name): {
            "smooth_quartz" => {}
        },
        get_name: "smooth_quartz",
        solid: true,
        cube: true,
    },
    SmoothStoneSlab {
        props: {},
        get_id: 11229,
        from_id(_id): 11229 => {},
        from_names(_name): {
            "smooth_stone_slab" => {}
        },
        get_name: "smooth_stone_slab[type=top]",
        transparent: true,
        cube: true,
    },
    QuartzSlab {
        props: {},
        get_id: 11283,
        from_id(_id): 11283 => {},
        from_names(_name): {
            "quartz_slab" => {}
        },
        get_name: "quartz_slab",
        transparent: true,
        cube: true,
    },
    Cauldron {
        props: {
            level: u8
        },
        get_id: level as u32 + 7398,
        from_id_offset: 7398,
        from_id(id): 7398..=7401 => {
            level: id as u8
        },
        from_names(_name): {
            "cauldron" => {
                level: 0
            },
            "water_cauldron" => {
                level: 3
            }
        },
        get_name: match level {
            0 => "cauldron",
            _ => "water_cauldron"
        },
        transparent: true,
        cube: false,
    },
    Composter {
        props: {
            level: u8
        },
        get_id: level as u32 + 19372,
        from_id_offset: 19372,
        from_id(id): 19372..=19380 => {
            level: id as u8
        },
        from_names(_name): {
            "composter" => {
                level: 0
            }
        },
        get_name: "composter",
        transparent: true,
        // FIXME: You can place repeaters and comparators on it, but not wires?
        cube: true,
    },
    Concrete {
        props: {
            color: BlockColorVariant
        },
        get_id: color.get_id() + 12728,
        from_id_offset: 12728,
        from_id(id): 12728..=12743 => {
            color: BlockColorVariant::from_id(id)
        },
        from_names(_name): {
            "white_concrete" => { color: BlockColorVariant::White },
            "orange_concrete" => { color: BlockColorVariant::Orange },
            "magenta_concrete" => { color: BlockColorVariant::Magenta },
            "light_blue_concrete" => { color: BlockColorVariant::LightBlue },
            "yellow_concrete" => { color: BlockColorVariant::Yellow },
            "lime_concrete" => { color: BlockColorVariant::Lime },
            "pink_concrete" => { color: BlockColorVariant::Pink },
            "gray_concrete" => { color: BlockColorVariant::Gray },
            "light_gray_concrete" => { color: BlockColorVariant::LightGray },
            "cyan_concrete" => { color: BlockColorVariant::Cyan },
            "purple_concrete" => { color: BlockColorVariant::Purple },
            "blue_concrete" => { color: BlockColorVariant::Blue },
            "brown_concrete" => { color: BlockColorVariant::Brown },
            "green_concrete" => { color: BlockColorVariant::Green },
            "red_concrete" => { color: BlockColorVariant::Red },
            "black_concrete" => { color: BlockColorVariant::Black }
        },
        get_name: match color {
            BlockColorVariant::White => "white_concrete",
            BlockColorVariant::Orange => "orange_concrete",
            BlockColorVariant::Magenta => "magenta_concrete",
            BlockColorVariant::LightBlue => "light_blue_concrete",
            BlockColorVariant::Yellow => "yellow_concrete",
            BlockColorVariant::Lime => "lime_concrete",
            BlockColorVariant::Pink => "pink_concrete",
            BlockColorVariant::Gray => "gray_concrete",
            BlockColorVariant::LightGray => "light_gray_concrete",
            BlockColorVariant::Cyan => "cyan_concrete",
            BlockColorVariant::Purple => "purple_concrete",
            BlockColorVariant::Blue => "blue_concrete",
            BlockColorVariant::Brown => "brown_concrete",
            BlockColorVariant::Green => "green_concrete",
            BlockColorVariant::Red => "red_concrete",
            BlockColorVariant::Black => "black_concrete",
        },
        solid: true,
        cube: true,
    },
    StainedGlass {
        props: {
            color: BlockColorVariant
        },
        get_id: color.get_id() + 5945,
        from_id_offset: 5945,
        from_id(id): 5945..=5960 => {
            color: BlockColorVariant::from_id(id)
        },
        from_names(_name): {
            "white_stained_glass" => { color: BlockColorVariant::White },
            "orange_stained_glass" => { color: BlockColorVariant::Orange },
            "magenta_stained_glass" => { color: BlockColorVariant::Magenta },
            "light_blue_stained_glass" => { color: BlockColorVariant::LightBlue },
            "yellow_stained_glass" => { color: BlockColorVariant::Yellow },
            "lime_stained_glass" => { color: BlockColorVariant::Lime },
            "pink_stained_glass" => { color: BlockColorVariant::Pink },
            "gray_stained_glass" => { color: BlockColorVariant::Gray },
            "light_gray_stained_glass" => { color: BlockColorVariant::LightGray },
            "cyan_stained_glass" => { color: BlockColorVariant::Cyan },
            "purple_stained_glass" => { color: BlockColorVariant::Purple },
            "blue_stained_glass" => { color: BlockColorVariant::Blue },
            "brown_stained_glass" => { color: BlockColorVariant::Brown },
            "green_stained_glass" => { color: BlockColorVariant::Green },
            "red_stained_glass" => { color: BlockColorVariant::Red },
            "black_stained_glass" => { color: BlockColorVariant::Black }
        },
        get_name: match color {
            BlockColorVariant::White => "white_stained_glass",
            BlockColorVariant::Orange => "orange_stained_glass",
            BlockColorVariant::Magenta => "magenta_stained_glass",
            BlockColorVariant::LightBlue => "light_blue_stained_glass",
            BlockColorVariant::Yellow => "yellow_stained_glass",
            BlockColorVariant::Lime => "lime_stained_glass",
            BlockColorVariant::Pink => "pink_stained_glass",
            BlockColorVariant::Gray => "gray_stained_glass",
            BlockColorVariant::LightGray => "light_gray_stained_glass",
            BlockColorVariant::Cyan => "cyan_stained_glass",
            BlockColorVariant::Purple => "purple_stained_glass",
            BlockColorVariant::Blue => "blue_stained_glass",
            BlockColorVariant::Brown => "brown_stained_glass",
            BlockColorVariant::Green => "green_stained_glass",
            BlockColorVariant::Red => "red_stained_glass",
            BlockColorVariant::Black => "black_stained_glass",
        },
        transparent: true,
        cube: true,
    },
    Terracotta {
        props: {},
        get_id: 10744,
        from_id(_id): 10744 => {},
        from_names(_name): {
            "terracotta" => {}
        },
        get_name: "terracotta",
        solid: true,
        cube: true,
    },
    ColoredTerracotta {
        props: {
            color: BlockColorVariant
        },
        get_id: color.get_id() + 9356,
        from_id_offset: 9356,
        from_id(id): 9356..=9371 => {
            color: BlockColorVariant::from_id(id)
        },
        from_names(_name): {
            "white_terracotta" => { color: BlockColorVariant::White },
            "orange_terracotta" => { color: BlockColorVariant::Orange },
            "magenta_terracotta" => { color: BlockColorVariant::Magenta },
            "light_blue_terracotta" => { color: BlockColorVariant::LightBlue },
            "yellow_terracotta" => { color: BlockColorVariant::Yellow },
            "lime_terracotta" => { color: BlockColorVariant::Lime },
            "pink_terracotta" => { color: BlockColorVariant::Pink },
            "gray_terracotta" => { color: BlockColorVariant::Gray },
            "light_gray_terracotta" => { color: BlockColorVariant::LightGray },
            "cyan_terracotta" => { color: BlockColorVariant::Cyan },
            "purple_terracotta" => { color: BlockColorVariant::Purple },
            "blue_terracotta" => { color: BlockColorVariant::Blue },
            "brown_terracotta" => { color: BlockColorVariant::Brown },
            "green_terracotta" => { color: BlockColorVariant::Green },
            "red_terracotta" => { color: BlockColorVariant::Red },
            "black_terracotta" => { color: BlockColorVariant::Black }
        },
        get_name: match color {
            BlockColorVariant::White => "white_terracotta",
            BlockColorVariant::Orange => "orange_terracotta",
            BlockColorVariant::Magenta => "magenta_terracotta",
            BlockColorVariant::LightBlue => "light_blue_terracotta",
            BlockColorVariant::Yellow => "yellow_terracotta",
            BlockColorVariant::Lime => "lime_terracotta",
            BlockColorVariant::Pink => "pink_terracotta",
            BlockColorVariant::Gray => "gray_terracotta",
            BlockColorVariant::LightGray => "light_gray_terracotta",
            BlockColorVariant::Cyan => "cyan_terracotta",
            BlockColorVariant::Purple => "purple_terracotta",
            BlockColorVariant::Blue => "blue_terracotta",
            BlockColorVariant::Brown => "brown_terracotta",
            BlockColorVariant::Green => "green_terracotta",
            BlockColorVariant::Red => "red_terracotta",
            BlockColorVariant::Black => "black_terracotta",
        },
        solid: true,
        cube: true,
    },
    Wool {
        props: {
            color: BlockColorVariant
        },
        get_id: color.get_id() + 2047,
        from_id_offset: 2047,
        from_id(id): 2047..=2062 => {
            color: BlockColorVariant::from_id(id)
        },
        from_names(_name): {
            "white_wool" => { color: BlockColorVariant::White },
            "orange_wool" => { color: BlockColorVariant::Orange },
            "magenta_wool" => { color: BlockColorVariant::Magenta },
            "light_blue_wool" => { color: BlockColorVariant::LightBlue },
            "yellow_wool" => { color: BlockColorVariant::Yellow },
            "lime_wool" => { color: BlockColorVariant::Lime },
            "pink_wool" => { color: BlockColorVariant::Pink },
            "gray_wool" => { color: BlockColorVariant::Gray },
            "light_gray_wool" => { color: BlockColorVariant::LightGray },
            "cyan_wool" => { color: BlockColorVariant::Cyan },
            "purple_wool" => { color: BlockColorVariant::Purple },
            "blue_wool" => { color: BlockColorVariant::Blue },
            "brown_wool" => { color: BlockColorVariant::Brown },
            "green_wool" => { color: BlockColorVariant::Green },
            "red_wool" => { color: BlockColorVariant::Red },
            "black_wool" => { color: BlockColorVariant::Black }
        },
        get_name: match color {
            BlockColorVariant::White => "white_wool",
            BlockColorVariant::Orange => "orange_wool",
            BlockColorVariant::Magenta => "magenta_wool",
            BlockColorVariant::LightBlue => "light_blue_wool",
            BlockColorVariant::Yellow => "yellow_wool",
            BlockColorVariant::Lime => "lime_wool",
            BlockColorVariant::Pink => "pink_wool",
            BlockColorVariant::Gray => "gray_wool",
            BlockColorVariant::LightGray => "light_gray_wool",
            BlockColorVariant::Cyan => "cyan_wool",
            BlockColorVariant::Purple => "purple_wool",
            BlockColorVariant::Blue => "blue_wool",
            BlockColorVariant::Brown => "brown_wool",
            BlockColorVariant::Green => "green_wool",
            BlockColorVariant::Red => "red_wool",
            BlockColorVariant::Black => "black_wool",
        },
        solid: true,
        cube: true,
    },
    IronTrapdoor {
        props: {
            facing: BlockDirection,
            half: TrapdoorHalf,
            powered: bool
        },
        get_id: {
            facing.get_id() * 16
                + half.get_id() * 8
                + !powered as u32 * 6
                + 10400
        },
        from_id_offset: 10400,
        from_id(id): 10400..=10462 => {
            facing: BlockDirection::from_id(id >> 4),
            half: TrapdoorHalf::from_id((id >> 3) & 1),
            powered: ((id >> 1) & 1) == 0
        },
        from_names(_name): {
            "iron_trapdoor" => {
                facing: Default::default(),
                half: TrapdoorHalf::Bottom,
                powered: false
            }
        },
        get_name: "iron_trapdoor",
    },
    NoteBlock {
        props: {
            instrument: Instrument,
            note: u32,
            powered: bool
        },
        get_id: {
            instrument.get_id() * 50
                + note * 2
                + !powered as u32
                + 538
        },
        from_id_offset: 538,
        from_id(id): 538..=1637 => {
            instrument: Instrument::from_id((id >> 1) / 25),
            note: (id >> 1) % 25,
            powered: (id & 1) == 0
        },
        from_names(_name): {
            "note_block" => {
                instrument: Instrument::Harp,
                note: 0,
                powered: false
            }
        },
        get_name: "note_block",
        solid: true,
        cube: true,
    },
    Clay {
        props: {},
        get_id: 5798,
        from_id(_id): 5798 => {},
        from_names(_name): {
            "clay" => {}
        },
        get_name: "clay",
        solid: true,
        cube: true,
    },
    GoldBlock {
        props: {},
        get_id: 2091,
        from_id(_id): 2091 => {},
        from_names(_name): {
            "gold_block" => {}
        },
        get_name: "gold_block",
        solid: true,
        cube: true,
    },
    PackedIce {
        props: {},
        get_id: 10746,
        from_id(_id): 10746 => {},
        from_names(_name): {
            "packed_ice" => {}
        },
        get_name: "packed_ice",
        solid: true,
        cube: true,
    },
    BoneBlock {
        props: {},
        get_id: 12546,
        from_id(_id): 12546..=12548 => {},
        from_names(_name): {
            "bone_block" => {}
        },
        get_name: "bone_block",
        solid: true,
        cube: true,
    },
    IronBlock {
        props: {},
        get_id: 2092,
        from_id(_id): 2092 => {},
        from_names(_name): {
            "iron_block" => {}
        },
        get_name: "iron_block",
        solid: true,
        cube: true,
    },
    SoulSand {
        props: {},
        get_id: 5850,
        from_id(_id): 5850 => {},
        from_names(_name): {
            "soul_sand" => {}
        },
        get_name: "soul_sand",
        solid: true,
        cube: true,
    },
    Pumpkin {
        props: {},
        get_id: 6813,
        from_id(_id): 6813 => {},
        from_names(_name): {
            "pumpkin" => {}
        },
        get_name: "pumpkin",
        solid: true,
        cube: true,
    },
    EmeraldBlock {
        props: {},
        get_id: 7665,
        from_id(_id): 7665 => {},
        from_names(_name): {
            "emerald_block" => {}
        },
        get_name: "emerald_block",
        solid: true,
        cube: true,
    },
    HayBlock {
        props: {},
        get_id: 10725,
        from_id(_id): 10725..=10727 => {},
        from_names(_name): {
            "hay_block" => {}
        },
        get_name: "hay_block",
        solid: true,
        cube: true,
    },
    Sand {
        props: {},
        get_id: 112,
        from_id(_id): 112 => {},
        from_names(_name): {
            "sand" => {}
        },
        get_name: "sand",
        solid: true,
        cube: true,
    },
    StoneBricks {
        props: {},
        get_id: 6537,
        from_id(_id): 6537 => {},
        from_names(_name): {
            "stone_bricks" => {}
        },
        get_name: "stone_bricks",
        solid: true,
        cube: true,
    },
    Unknown {
        props: {
            id: u32
        },
        get_id: id,
        from_id(id): _ => { id: id },
        from_names(name): {},
        get_name: "unknown",
        solid: true,
        cube: true,
    }
}
