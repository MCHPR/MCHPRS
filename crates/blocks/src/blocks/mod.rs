mod props;

pub use crate::generated::Block;
use crate::{BlockDirection, BlockFacing, BlockProperty};
use mchprs_proc_macros::BlockTransform;
pub use props::*;

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

pub(crate) trait BlockTransform {
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
    BlockFacing,
    TrapdoorHalf,
    LeverFace,
    ComparatorMode,
    Instrument,
    SlabType,
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
    pub fn is_sign(self) -> bool {
        matches!(
            self,
            Block::OakSign { .. }
                | Block::SpruceSign { .. }
                | Block::BirchSign { .. }
                | Block::AcaciaSign { .. }
                | Block::JungleSign { .. }
                | Block::DarkOakSign { .. }
                | Block::CrimsonSign { .. }
                | Block::WarpedSign { .. }
                | Block::BambooSign { .. }
                | Block::CherrySign { .. }
                | Block::MangroveSign { .. }
        )
    }

    pub fn is_wall_sign(self) -> bool {
        matches!(
            self,
            Block::OakWallSign { .. }
                | Block::SpruceWallSign { .. }
                | Block::BirchWallSign { .. }
                | Block::AcaciaWallSign { .. }
                | Block::JungleWallSign { .. }
                | Block::DarkOakWallSign { .. }
                | Block::CrimsonWallSign { .. }
                | Block::WarpedWallSign { .. }
                | Block::BambooWallSign { .. }
                | Block::CherryWallSign { .. }
                | Block::MangroveWallSign { .. }
        )
    }

    pub fn has_block_entity(self) -> bool {
        self.is_sign()
            || self.is_wall_sign()
            || matches!(
                self,
                Block::Comparator { .. }
                    | Block::Barrel { .. }
                    | Block::Furnace { .. }
                    | Block::Hopper { .. }
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

    pub(crate) fn complex_rotate(&mut self, amt: RotateAmt) {
        match self {
            Block::RedstoneWire(wire) => wire.rotate(amt),
            _ => unreachable!(),
        }
    }

    pub(crate) fn complex_flip(&mut self, dir: FlipDirection) {
        match self {
            Block::RedstoneWire(wire) => wire.flip(dir),
            _ => unreachable!(),
        }
    }

    pub fn get_sign_rotation(self) -> Option<u8> {
        Some(match self {
            Block::OakSign { rotation, .. }
            | Block::SpruceSign { rotation, .. }
            | Block::BirchSign { rotation, .. }
            | Block::AcaciaSign { rotation, .. }
            | Block::JungleSign { rotation, .. }
            | Block::DarkOakSign { rotation, .. }
            | Block::CrimsonSign { rotation, .. }
            | Block::WarpedSign { rotation, .. } => rotation,
            _ => return None,
        })
    }

    pub fn get_wall_sign_facing(self) -> Option<BlockDirection> {
        Some(match self {
            Block::OakWallSign { facing, .. }
            | Block::SpruceWallSign { facing, .. }
            | Block::BirchWallSign { facing, .. }
            | Block::AcaciaWallSign { facing, .. }
            | Block::JungleWallSign { facing, .. }
            | Block::DarkOakWallSign { facing, .. }
            | Block::CrimsonWallSign { facing, .. }
            | Block::WarpedWallSign { facing, .. } => facing,
            _ => return None,
        })
    }

    pub(crate) fn is_solid_dynamic(self) -> bool {
        match self {
            Block::SmoothStoneSlab { ty, .. } | Block::QuartzSlab { ty, .. } => {
                ty == SlabType::Double
            }
            _ => unreachable!(),
        }
    }

    pub(crate) fn is_transparent_dynamic(self) -> bool {
        match self {
            Block::SmoothStoneSlab { ty, .. } | Block::QuartzSlab { ty, .. } => {
                ty != SlabType::Double
            }
            _ => unreachable!(),
        }
    }

    pub(crate) fn is_cube_dynamic(self) -> bool {
        match self {
            Block::SmoothStoneSlab { ty, .. } | Block::QuartzSlab { ty, .. } => ty == SlabType::Top,
            _ => unreachable!(),
        }
    }

    pub fn get_pressure_plate_powered(&mut self) -> Option<&mut bool> {
        match self {
            Block::OakPressurePlate { powered }
            | Block::SprucePressurePlate { powered }
            | Block::BirchPressurePlate { powered }
            | Block::JunglePressurePlate { powered }
            | Block::AcaciaPressurePlate { powered }
            | Block::DarkOakPressurePlate { powered }
            | Block::MangrovePressurePlate { powered }
            | Block::CherryPressurePlate { powered }
            | Block::BambooPressurePlate { powered }
            | Block::CrimsonPressurePlate { powered }
            | Block::WarpedPressurePlate { powered }
            | Block::StonePressurePlate { powered }
            | Block::PolishedBlackstonePressurePlate { powered } => Some(powered),
            _ => None,
        }
    }
}

#[test]
fn repeater_id_test() {
    let original = Block::Repeater(Repeater::new(3, BlockDirection::West, true, false));
    let id = original.get_id();
    assert_eq!(id, 4141);
    let new = Block::from_id(id);
    assert_eq!(new, original);
}

#[test]
fn comparator_id_test() {
    let original = Block::Comparator(Comparator::new(
        BlockDirection::West,
        ComparatorMode::Subtract,
        false,
    ));
    let id = original.get_id();
    assert_eq!(id, 6895);
    let new = Block::from_id(id);
    assert_eq!(new, original);
}
