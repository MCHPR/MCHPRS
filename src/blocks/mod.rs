mod redstone;

use crate::items::{ActionResult, Item, UseOnBlockContext};
use crate::world::TickPriority;
use crate::world::World;
use redstone::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignBlockEntity {
    pub rows: [String; 4],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlockEntity {
    Comparator { output_strength: u8 },
    Container { comparator_override: u8 },
    Sign(Box<SignBlockEntity>),
}

macro_rules! nbt_unwrap_val {
    // I'm not sure if path is the right type here.
    // It works though!
    ($e:expr, $p:path) => {
        match $e {
            $p(val) => val,
            _ => return None,
        }
    };
}

impl BlockEntity {
    fn load_container(slots_nbt: &[nbt::Value], num_slots: u8) -> Option<BlockEntity> {
        use nbt::Value;
        let mut fullness_sum: f32 = 0.0;
        for item in slots_nbt {
            let item_compound = nbt_unwrap_val!(item, Value::Compound);
            let count = nbt_unwrap_val!(item_compound["Count"], Value::Byte);
            let namespaced_name = nbt_unwrap_val!(
                item_compound
                    .get("Id")
                    .or_else(|| item_compound.get("id"))?,
                Value::String
            );
            let item_type = Item::from_name(namespaced_name.split(':').last()?);
            fullness_sum += count as f32 / item_type.map(Item::max_stack_size).unwrap_or(64) as f32;
        }
        Some(BlockEntity::Container {
            comparator_override: (1.0 + (fullness_sum / num_slots as f32) * 14.0).floor() as u8,
        })
    }

    pub fn from_nbt(nbt: &HashMap<String, nbt::Value>) -> Option<BlockEntity> {
        use nbt::Value;
        let id = nbt_unwrap_val!(&nbt.get("Id").or_else(|| nbt.get("id"))?, Value::String);
        match id.as_ref() {
            "minecraft:comparator" => Some(BlockEntity::Comparator {
                output_strength: *nbt_unwrap_val!(&nbt["OutputSignal"], Value::Int) as u8,
            }),
            "minecraft:furnace" => {
                BlockEntity::load_container(nbt_unwrap_val!(&nbt["Items"], Value::List), 3)
            }
            "minecraft:barrel" => {
                BlockEntity::load_container(nbt_unwrap_val!(&nbt["Items"], Value::List), 27)
            }
            "minecraft:hopper" => {
                BlockEntity::load_container(nbt_unwrap_val!(&nbt["Items"], Value::List), 5)
            }
            "minecraft:sign" => Some({
                BlockEntity::Sign(Box::new(SignBlockEntity {
                    rows: [
                        // This cloning is really dumb
                        nbt_unwrap_val!(nbt["Text1"].clone(), Value::String),
                        nbt_unwrap_val!(nbt["Text2"].clone(), Value::String),
                        nbt_unwrap_val!(nbt["Text3"].clone(), Value::String),
                        nbt_unwrap_val!(nbt["Text4"].clone(), Value::String),
                    ],
                }))
            }),
            _ => None,
        }
    }

    pub fn to_nbt(&self, pos: BlockPos) -> Option<nbt::Blob> {
        use nbt::Value;
        let blob = match self {
            BlockEntity::Sign(sign) => Some({
                let mut blob = nbt::Blob::new();
                let [r1, r2, r3, r4] = sign.rows.clone();
                let _ = blob.insert("Text1", Value::String(r1));
                let _ = blob.insert("Text2", Value::String(r2));
                let _ = blob.insert("Text3", Value::String(r3));
                let _ = blob.insert("Text4", Value::String(r4));
                let _ = blob.insert("id", Value::String("minecraft:sign".to_owned()));
                blob
            }),
            _ => None,
        };
        blob.map(|mut nbt| {
            let _ = nbt.insert("x", Value::Int(pos.x));
            let _ = nbt.insert("y", Value::Int(pos.y as i32));
            let _ = nbt.insert("z", Value::Int(pos.z));
            nbt
        })
    }
}

#[derive(PartialEq, Eq, Copy, Clone, Debug, Serialize, Deserialize, Hash)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPos {
    pub fn new(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos { x, y, z }
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

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum BlockDirection {
    North,
    South,
    East,
    West,
}

impl BlockDirection {
    fn opposite(self) -> BlockDirection {
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

    fn from_str(name: &str) -> BlockDirection {
        match name {
            "north" => BlockDirection::North,
            "south" => BlockDirection::South,
            "east" => BlockDirection::East,
            _ => BlockDirection::West,
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

    fn rotate(self) -> BlockDirection {
        use BlockDirection::*;
        match self {
            North => East,
            East => South,
            South => West,
            West => North,
        }
    }

    fn rotate_ccw(self) -> BlockDirection {
        use BlockDirection::*;
        match self {
            North => West,
            West => South,
            South => East,
            East => North,
        }
    }
}

impl Default for BlockDirection {
    fn default() -> Self {
        BlockDirection::West
    }
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
            _ => BlockFace::West,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum BlockFacing {
    North,
    East,
    South,
    West,
    Up,
    Down,
}

impl BlockFacing {
    fn from_id(id: u32) -> BlockFacing {
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

    fn get_id(self) -> u32 {
        match self {
            BlockFacing::North => 0,
            BlockFacing::East => 1,
            BlockFacing::South => 2,
            BlockFacing::West => 3,
            BlockFacing::Up => 4,
            BlockFacing::Down => 5,
        }
    }

    fn from_str(name: &str) -> BlockFacing {
        match name {
            "north" => BlockFacing::North,
            "south" => BlockFacing::South,
            "east" => BlockFacing::East,
            "west" => BlockFacing::West,
            "up" => BlockFacing::Up,
            _ => BlockFacing::Down,
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
}

impl Default for BlockFacing {
    fn default() -> Self {
        BlockFacing::West
    }
}

impl BlockFace {
    pub fn values() -> [BlockFace; 6] {
        use BlockFace::*;
        [Top, Bottom, North, South, East, West]
    }

    fn is_horizontal(self) -> bool {
        use BlockFace::*;
        match self {
            North | South | East | West => true,
            _ => false,
        }
    }

    fn to_direction(self) -> BlockDirection {
        match self {
            BlockFace::North => BlockDirection::North,
            BlockFace::South => BlockDirection::South,
            BlockFace::East => BlockDirection::East,
            BlockFace::West => BlockDirection::West,
            _ => BlockDirection::West,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
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

impl Block {
    pub fn has_block_entity(self) -> bool {
        match self {
            Block::RedstoneComparator { .. }
            | Block::Barrel { .. }
            | Block::Furnace { .. }
            | Block::Hopper { .. }
            | Block::Sign { .. }
            | Block::WallSign { .. } => true,
            _ => false,
        }
    }

    fn has_comparator_override(self) -> bool {
        match self {
            Block::Barrel { .. } | Block::Furnace { .. } | Block::Hopper { .. } => true,
            _ => false,
        }
    }

    fn get_comparator_override(self, world: &dyn World, pos: BlockPos) -> u8 {
        match self {
            Block::Barrel { .. } | Block::Furnace { .. } | Block::Hopper { .. } => {
                if let Some(BlockEntity::Container {
                    comparator_override,
                }) = world.get_block_entity(pos)
                {
                    *comparator_override
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    fn is_diode(self) -> bool {
        match self {
            Block::RedstoneRepeater { .. } | Block::RedstoneComparator { .. } => true,
            _ => false,
        }
    }

    pub fn can_place_block_in(self) -> bool {
        match self.get_id() {
            0 => true,           // Air
            9129..=9130 => true, // Void and Cave air
            34..=49 => true,     // Water
            50..=65 => true,     // Lava
            1341 => true,        // Grass
            1342 => true,        // Fern
            1343 => true,        // Dead bush
            1344 => true,        // Seagrass
            1345..=1346 => true, // Tall Seagrass
            7357..=7358 => true, // Tall Grass
            7359..=7360 => true, // Tall Fern
            _ => false,
        }
    }

    pub fn on_use(
        self,
        world: &mut dyn World,
        pos: BlockPos,
        item_in_hand: Option<Item>,
    ) -> ActionResult {
        match self {
            Block::RedstoneRepeater { repeater } => {
                let mut repeater = repeater;
                repeater.delay += 1;
                if repeater.delay > 4 {
                    repeater.delay -= 4;
                }
                world.set_block(pos, Block::RedstoneRepeater { repeater });
                ActionResult::Success
            }
            Block::RedstoneComparator { comparator } => {
                let mut comparator = comparator;
                comparator.mode = comparator.mode.toggle();
                comparator.tick(world, pos);
                world.set_block(pos, Block::RedstoneComparator { comparator });
                ActionResult::Success
            }
            Block::Lever { mut lever } => {
                lever.powered = !lever.powered;
                world.set_block(pos, Block::Lever { lever });
                Block::update_surrounding_blocks(world, pos);
                match lever.face {
                    LeverFace::Ceiling => {
                        Block::update_surrounding_blocks(world, pos.offset(BlockFace::Top))
                    }
                    LeverFace::Floor => {
                        Block::update_surrounding_blocks(world, pos.offset(BlockFace::Bottom))
                    }
                    LeverFace::Wall => Block::update_surrounding_blocks(
                        world,
                        pos.offset(lever.facing.opposite().block_face()),
                    ),
                }
                ActionResult::Success
            }
            Block::StoneButton { mut button } => {
                if !button.powered {
                    button.powered = true;
                    world.set_block(pos, Block::StoneButton { button });
                    world.schedule_tick(pos, 10, TickPriority::Normal);
                    Block::update_surrounding_blocks(world, pos);
                    match button.face {
                        ButtonFace::Ceiling => {
                            Block::update_surrounding_blocks(world, pos.offset(BlockFace::Top))
                        }
                        ButtonFace::Floor => {
                            Block::update_surrounding_blocks(world, pos.offset(BlockFace::Bottom))
                        }
                        ButtonFace::Wall => Block::update_surrounding_blocks(
                            world,
                            pos.offset(button.facing.opposite().block_face()),
                        ),
                    }
                }
                ActionResult::Success
            }
            Block::RedstoneWire { wire } => wire.on_use(world, pos),
            Block::SeaPickle { pickles } => {
                if let Some(Item::SeaPickle {}) = item_in_hand {
                    if pickles < 4 {
                        world.set_block(
                            pos,
                            Block::SeaPickle {
                                pickles: pickles + 1,
                            },
                        );
                    }
                }
                ActionResult::Success
            }
            _ => ActionResult::Pass,
        }
    }

    pub fn get_state_for_placement(
        world: &dyn World,
        pos: BlockPos,
        item: Item,
        context: &UseOnBlockContext,
    ) -> Block {
        let block = match item {
            Item::Glass {} => Block::Glass {},
            Item::Sandstone {} => Block::Sandstone {},
            Item::SeaPickle {} => Block::SeaPickle { pickles: 1 },
            Item::Wool { color } => Block::Wool { color },
            Item::Furnace {} => Block::Furnace {},
            Item::Lever {} => {
                let lever_face = match context.block_face {
                    BlockFace::Top => LeverFace::Floor,
                    BlockFace::Bottom => LeverFace::Ceiling,
                    _ => LeverFace::Wall,
                };
                let facing = if lever_face == LeverFace::Wall {
                    context.block_face.to_direction()
                } else {
                    context.player_direction
                };
                Block::Lever {
                    lever: Lever::new(lever_face, facing, false),
                }
            }
            Item::RedstoneTorch {} => match context.block_face {
                BlockFace::Top => Block::RedstoneTorch { lit: true },
                BlockFace::Bottom => Block::RedstoneTorch { lit: true },
                face => Block::RedstoneWallTorch {
                    lit: true,
                    facing: face.to_direction(),
                },
            },
            Item::StoneButton {} => {
                let button_face = match context.block_face {
                    BlockFace::Top => ButtonFace::Floor,
                    BlockFace::Bottom => ButtonFace::Ceiling,
                    _ => ButtonFace::Wall,
                };
                let facing = if button_face == ButtonFace::Wall {
                    context.block_face.to_direction()
                } else {
                    context.player_direction
                };
                Block::StoneButton {
                    button: StoneButton::new(button_face, facing, false),
                }
            }
            Item::RedstoneLamp {} => Block::RedstoneLamp {
                lit: Block::redstone_lamp_should_be_lit(world, pos),
            },
            Item::RedstoneBlock {} => Block::RedstoneBlock {},
            Item::Hopper {} => Block::Hopper {},
            Item::Terracotta { color } => Block::Terracotta { color },
            Item::Concrete { color } => Block::Concrete { color },
            Item::Repeater {} => Block::RedstoneRepeater {
                repeater: RedstoneRepeater::get_state_for_placement(
                    world,
                    pos,
                    context.player_direction.opposite(),
                ),
            },
            Item::Comparator {} => Block::RedstoneComparator {
                comparator: RedstoneComparator::new(
                    context.player_direction.opposite(),
                    ComparatorMode::Compare,
                    false,
                ),
            },
            Item::Sign { sign_type } => match context.block_face {
                BlockFace::Bottom => Block::Air {},
                BlockFace::Top => Block::Sign {
                    sign_type,
                    rotation: (((180.0 + context.player_yaw) * 16.0 / 360.0) + 0.5).floor() as u32
                        & 15,
                },
                _ => Block::WallSign {
                    sign_type,
                    facing: context.block_face.to_direction(),
                },
            },
            Item::Redstone {} => Block::RedstoneWire {
                wire: RedstoneWire::get_state_for_placement(world, pos),
            },
            Item::Barrel {} => Block::Barrel {},
            Item::Target {} => Block::Target {},
            Item::StainedGlass { color } => Block::StainedGlass {
                color
            },
            Item::SmoothStoneSlab {} => Block::SmoothStoneSlab {},
            Item::QuartzSlab {} => Block::QuartzSlab {},
            _ => Block::Air {},
        };
        if block.is_valid_position(world, pos) {
            block
        } else {
            Block::Air {}
        }
    }

    pub fn place_in_world(self, world: &mut dyn World, pos: BlockPos, nbt: &Option<nbt::Blob>) {
        if self.has_block_entity() {
            if let Some(nbt) = nbt {
                if let nbt::Value::Compound(compound) = &nbt["BlockEntityTag"] {
                    if let Some(block_entity) = BlockEntity::from_nbt(compound) {
                        world.set_block_entity(pos, block_entity);
                    }
                }
            };
        }
        match self {
            Block::RedstoneRepeater { .. } => {
                // TODO: Queue repeater tick
                world.set_block(pos, self);
                Block::change_surrounding_blocks(world, pos);
                Block::update_surrounding_blocks(world, pos);
            }
            Block::RedstoneWire { .. } => {
                world.set_block(pos, self);
                Block::change_surrounding_blocks(world, pos);
                Block::update_wire_neighbors(world, pos);
            }
            _ => {
                world.set_block(pos, self);
                Block::change_surrounding_blocks(world, pos);
                Block::update_surrounding_blocks(world, pos);
            }
        }
    }

    pub fn destroy(self, world: &mut dyn World, pos: BlockPos) {
        if self.has_block_entity() {
            world.delete_block_entity(pos);
        }

        match self {
            Block::RedstoneWire { .. } => {
                world.set_block(pos, Block::Air {});
                Block::change_surrounding_blocks(world, pos);
                Block::update_wire_neighbors(world, pos);
            }
            Block::Lever { lever } => {
                world.set_block(pos, Block::Air {});
                // This is a horrible idea, don't do this.
                // One day this will be fixed, but for now... too bad!
                match lever.face {
                    LeverFace::Ceiling => {
                        Block::change_surrounding_blocks(world, pos.offset(BlockFace::Top));
                        Block::update_surrounding_blocks(world, pos.offset(BlockFace::Top));
                    }
                    LeverFace::Floor => {
                        Block::change_surrounding_blocks(world, pos.offset(BlockFace::Bottom));
                        Block::update_surrounding_blocks(world, pos.offset(BlockFace::Bottom));
                    }
                    LeverFace::Wall => {
                        Block::change_surrounding_blocks(
                            world,
                            pos.offset(lever.facing.opposite().block_face()),
                        );
                        Block::update_surrounding_blocks(
                            world,
                            pos.offset(lever.facing.opposite().block_face()),
                        );
                    }
                }
            }
            _ => {
                world.set_block(pos, Block::Air {});
                Block::change_surrounding_blocks(world, pos);
                Block::update_surrounding_blocks(world, pos);
            }
        }
    }

    fn update(self, world: &mut dyn World, pos: BlockPos) {
        match self {
            Block::RedstoneWire { wire } => {
                wire.on_neighbor_updated(world, pos);
            }
            Block::RedstoneTorch { lit } => {
                if lit == Block::torch_should_be_off(world, pos) && !world.pending_tick_at(pos) {
                    world.schedule_tick(pos, 1, TickPriority::Normal);
                }
            }
            Block::RedstoneWallTorch { lit, facing } => {
                if lit == Block::wall_torch_should_be_off(world, pos, facing)
                    && !world.pending_tick_at(pos)
                {
                    world.schedule_tick(pos, 1, TickPriority::Normal);
                }
            }
            Block::RedstoneRepeater { repeater } => {
                repeater.on_neighbor_updated(world, pos);
            }
            Block::RedstoneComparator { comparator } => {
                comparator.update(world, pos);
            }
            Block::RedstoneLamp { lit } => {
                let should_be_lit = Block::redstone_lamp_should_be_lit(world, pos);
                if lit && !should_be_lit {
                    world.schedule_tick(pos, 2, TickPriority::Normal);
                } else if !lit && should_be_lit {
                    world.set_block(pos, Block::RedstoneLamp { lit: true });
                }
            }
            _ => {}
        }
    }

    pub fn tick(self, world: &mut dyn World, pos: BlockPos) {
        match self {
            Block::RedstoneRepeater { repeater } => {
                repeater.tick(world, pos);
            }
            Block::RedstoneComparator { comparator } => {
                comparator.tick(world, pos);
            }
            Block::RedstoneTorch { lit } => {
                let should_be_off = Block::torch_should_be_off(world, pos);
                if lit && should_be_off {
                    world.set_block(pos, Block::RedstoneTorch { lit: false });
                    Block::update_surrounding_blocks(world, pos);
                } else if !lit && !should_be_off {
                    world.set_block(pos, Block::RedstoneTorch { lit: true });
                    Block::update_surrounding_blocks(world, pos);
                }
            }
            Block::RedstoneWallTorch { lit, facing } => {
                let should_be_off = Block::wall_torch_should_be_off(world, pos, facing);
                if lit && should_be_off {
                    world.set_block(pos, Block::RedstoneWallTorch { lit: false, facing });
                    Block::update_surrounding_blocks(world, pos);
                } else if !lit && !should_be_off {
                    world.set_block(pos, Block::RedstoneWallTorch { lit: true, facing });
                    Block::update_surrounding_blocks(world, pos);
                }
            }
            Block::RedstoneLamp { lit } => {
                let should_be_lit = Block::redstone_lamp_should_be_lit(world, pos);
                if lit && !should_be_lit {
                    world.set_block(pos, Block::RedstoneLamp { lit: false });
                }
            }
            Block::StoneButton { mut button } => {
                if button.powered {
                    button.powered = false;
                    world.set_block(pos, Block::StoneButton { button });
                    Block::update_surrounding_blocks(world, pos);
                    match button.face {
                        ButtonFace::Ceiling => {
                            Block::update_surrounding_blocks(world, pos.offset(BlockFace::Top))
                        }
                        ButtonFace::Floor => {
                            Block::update_surrounding_blocks(world, pos.offset(BlockFace::Bottom))
                        }
                        ButtonFace::Wall => Block::update_surrounding_blocks(
                            world,
                            pos.offset(button.facing.opposite().block_face()),
                        ),
                    }
                }
            }
            _ => {}
        }
    }

    pub fn is_valid_position(self, world: &dyn World, pos: BlockPos) -> bool {
        match self {
            Block::RedstoneWire { .. }
            | Block::RedstoneComparator { .. }
            | Block::RedstoneRepeater { .. }
            | Block::RedstoneTorch { .. } => {
                let bottom_block = world.get_block(pos.offset(BlockFace::Bottom));
                bottom_block.is_cube()
            }
            Block::RedstoneWallTorch { facing, .. } => {
                let parent_block = world.get_block(pos.offset(facing.opposite().block_face()));
                parent_block.is_cube()
            }
            Block::Lever { lever } => match lever.face {
                LeverFace::Floor => {
                    let bottom_block = world.get_block(pos.offset(BlockFace::Bottom));
                    bottom_block.is_cube()
                }
                LeverFace::Ceiling => {
                    let top_block = world.get_block(pos.offset(BlockFace::Top));
                    top_block.is_cube()
                }
                LeverFace::Wall => {
                    let parent_block =
                        world.get_block(pos.offset(lever.facing.opposite().block_face()));
                    parent_block.is_cube()
                }
            },
            Block::StoneButton { button } => match button.face {
                ButtonFace::Floor => {
                    let bottom_block = world.get_block(pos.offset(BlockFace::Bottom));
                    bottom_block.is_cube()
                }
                ButtonFace::Ceiling => {
                    let top_block = world.get_block(pos.offset(BlockFace::Top));
                    top_block.is_cube()
                }
                ButtonFace::Wall => {
                    let parent_block =
                        world.get_block(pos.offset(button.facing.opposite().block_face()));
                    parent_block.is_cube()
                }
            },
            _ => true,
        }
    }

    fn change(self, world: &mut dyn World, pos: BlockPos, direction: BlockFace) {
        if !self.is_valid_position(world, pos) {
            self.destroy(world, pos);
            return;
        }
        match self {
            Block::RedstoneWire { wire } => {
                let new_state = wire.on_neighbor_changed(world, pos, direction);
                if world.set_block(pos, Block::RedstoneWire { wire: new_state }) {
                    Block::update_wire_neighbors(world, pos);
                }
            }
            _ => {}
        }
    }

    fn update_wire_neighbors(world: &mut dyn World, pos: BlockPos) {
        for direction in &BlockFace::values() {
            let neighbor_pos = pos.offset(*direction);
            let block = world.get_block(neighbor_pos);
            block.update(world, neighbor_pos);
            for n_direction in &BlockFace::values() {
                let n_neighbor_pos = neighbor_pos.offset(*n_direction);
                let block = world.get_block(n_neighbor_pos);
                block.update(world, n_neighbor_pos);
            }
        }
    }

    fn update_surrounding_blocks(world: &mut dyn World, pos: BlockPos) {
        for direction in &BlockFace::values() {
            let neighbor_pos = pos.offset(*direction);
            let block = world.get_block(neighbor_pos);
            block.update(world, neighbor_pos);

            // Also update diagonal blocks

            let up_pos = neighbor_pos.offset(BlockFace::Top);
            let up_block = world.get_block(up_pos);
            up_block.update(world, up_pos);

            let down_pos = neighbor_pos.offset(BlockFace::Bottom);
            let down_block = world.get_block(down_pos);
            down_block.update(world, down_pos);
        }
    }

    fn change_surrounding_blocks(world: &mut dyn World, pos: BlockPos) {
        for direction in &BlockFace::values() {
            let neighbor_pos = pos.offset(*direction);
            let block = world.get_block(neighbor_pos);
            block.change(world, neighbor_pos, *direction);

            // Also change diagonal blocks

            let up_pos = neighbor_pos.offset(BlockFace::Top);
            let up_block = world.get_block(up_pos);
            up_block.change(world, up_pos, *direction);

            let down_pos = neighbor_pos.offset(BlockFace::Bottom);
            let down_block = world.get_block(down_pos);
            down_block.change(world, down_pos, *direction);
        }
    }

    pub fn set_property(&mut self, key: &str, val: &str) {
        // Macros might be able to help here
        match self {
            Block::RedstoneWire { wire } if key == "north" => {
                wire.north = RedstoneWireSide::from_str(val);
            }
            Block::RedstoneWire { wire } if key == "south" => {
                wire.south = RedstoneWireSide::from_str(val);
            }
            Block::RedstoneWire { wire } if key == "east" => {
                wire.east = RedstoneWireSide::from_str(val);
            }
            Block::RedstoneWire { wire } if key == "west" => {
                wire.west = RedstoneWireSide::from_str(val);
            }
            Block::RedstoneWire { wire } if key == "power" => {
                wire.power = val.parse::<u8>().unwrap_or_default();
            }
            Block::RedstoneLamp { lit } if key == "lit" => {
                *lit = val.parse::<bool>().unwrap_or_default();
            }
            Block::RedstoneTorch { lit } | Block::RedstoneWallTorch { lit, .. } if key == "lit" => {
                *lit = val.parse::<bool>().unwrap_or_default();
            }
            Block::RedstoneWallTorch { facing, .. } if key == "facing" => {
                *facing = BlockDirection::from_str(val);
            }
            Block::RedstoneRepeater { repeater } if key == "facing" => {
                repeater.facing = BlockDirection::from_str(val);
            }
            Block::RedstoneRepeater { repeater } if key == "delay" => {
                repeater.delay = val.parse::<u8>().unwrap_or(1);
            }
            Block::RedstoneRepeater { repeater } if key == "powered" => {
                repeater.powered = val.parse::<bool>().unwrap_or_default();
            }
            Block::RedstoneRepeater { repeater } if key == "locked" => {
                repeater.locked = val.parse::<bool>().unwrap_or_default();
            }
            Block::RedstoneComparator { comparator } if key == "facing" => {
                comparator.facing = BlockDirection::from_str(val);
            }
            Block::RedstoneComparator { comparator } if key == "mode" => {
                comparator.mode = ComparatorMode::from_str(val);
            }
            Block::RedstoneComparator { comparator } if key == "powered" => {
                comparator.powered = val.parse::<bool>().unwrap_or_default();
            }
            Block::Lever { lever } if key == "face" => {
                lever.face = LeverFace::from_str(val);
            }
            Block::Lever { lever } if key == "facing" => {
                lever.facing = BlockDirection::from_str(val);
            }
            Block::Lever { lever } if key == "powered" => {
                lever.powered = val.parse::<bool>().unwrap_or_default();
            }
            Block::StoneButton { button } if key == "face" => {
                button.face = ButtonFace::from_str(val);
            }
            Block::StoneButton { button } if key == "facing" => {
                button.facing = BlockDirection::from_str(val);
            }
            Block::StoneButton { button } if key == "powered" => {
                button.powered = val.parse::<bool>().unwrap_or_default();
            }
            Block::TripwireHook { direction, .. } if key == "facing" => {
                *direction = BlockDirection::from_str(val);
            }
            Block::Observer { facing } if key == "facing" => {
                *facing = BlockFacing::from_str(val);
            }
            Block::WallSign { facing, .. } if key == "facing" => {
                *facing = BlockDirection::from_str(val);
            }
            Block::Sign { rotation, .. } if key == "rotation" => {
                *rotation = val.parse::<u32>().unwrap_or_default();
            }
            _ => {}
        }
    }
}

#[test]
fn repeater_id_test() {
    let original = Block::RedstoneRepeater {
        repeater: RedstoneRepeater::new(3, BlockDirection::West, true, false),
    };
    let id = original.get_id();
    assert_eq!(id, 4072);
    let new = Block::from_id(id);
    assert_eq!(new, original);
}

#[test]
fn comparator_id_test() {
    let original = Block::RedstoneComparator {
        comparator: RedstoneComparator::new(BlockDirection::West, ComparatorMode::Subtract, false),
    };
    let id = original.get_id();
    assert_eq!(id, 6693);
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
                $( solid: $solid:literal, )?
                $( transparent: $transparent:literal, )?
                $( cube: $cube:literal, )?
            }
        ),*
    ) => {
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub enum Block {
            $(
                $name {
                    $(
                        $prop_name: $prop_type,
                    )*
                }
            ),*
        }

        impl Block {
            fn is_solid(self) -> bool {
                match self {
                    $(
                        $( Block::$name { .. } => $solid, )?
                    )*
                    _ => false
                }
            }

            fn is_transparent(self) -> bool {
                match self {
                    $(
                        $( Block::$name { .. } => $transparent, )?
                    )*
                    _ => false
                }
            }

            fn is_cube(self) -> bool {
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
        }
    }
}

blocks! {
    Air {
        props: {},
        get_id: 0,
        from_id(id): 0 => {},
        from_names(name): {
            "air" => {}
        },
    },
    Glass {
        props: {},
        get_id: 231,
        from_id(id): 231 => {},
        from_names(name): {
            "glass" => {}
        },
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
                + 2058
        },
        from_id_offset: 2058,
        from_id(id): 2058..=3353 => {
            wire: RedstoneWire::new(
                RedstoneWireSide::from_id(id % 432 / 144),
                RedstoneWireSide::from_id(id % 9 / 3),
                RedstoneWireSide::from_id(id / 432),
                RedstoneWireSide::from_id(id % 3),
                (id % 144 / 9) as u8,
            )
        },
        from_names(name): {
            "redstone_wire" => {
                wire: Default::default()
            }
        },
    },
    WallSign {
        props: {
            sign_type: u32,
            facing: BlockDirection
        },
        get_id: (sign_type << 3) + (facing.get_id() << 1) + 3736,
        from_id_offset: 3381,
        from_id(id): 3735..=3781 => {
            sign_type: id >> 3,
            facing: BlockDirection::from_id((id & 0b110) >> 1)
        },
        from_names(name): {
            "oak_wall_sign" => {
                sign_type: 0,
                facing: Default::default()
            },
            "spruce_wall_sign" => {
                sign_type: 1,
                facing: Default::default()
            },
            "birch_wall_sign" => {
                sign_type: 2,
                facing: Default::default()
            },
            "jungle_wall_sign" => {
                sign_type: 3,
                facing: Default::default()
            },
            "acacia_wall_sign" => {
                sign_type: 4,
                facing: Default::default()
            },
            "dark_oak_wall_sign" => {
                sign_type: 5,
                facing: Default::default()
            }
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
                + 3783
        },
        from_id_offset: 3783,
        from_id(id): 3783..=3806 => {
            lever: Lever::new(
                LeverFace::from_id(id >> 3),
                BlockDirection::from_id((id >> 1) & 0b11),
                (id & 1) == 0
            )
        },
        from_names(name): {
            "lever" => {
                lever: Default::default()
            }
        },
    },
    StoneButton {
        props: {
            button: StoneButton
        },
        get_id: {
            (button.face.get_id() << 3)
                + (button.facing.get_id() << 1)
                + !button.powered as u32
                + 3897
        },
        from_id_offset: 3897,
        from_id(id): 0 => {
            button: StoneButton::new(ButtonFace::from_id(id >> 3), BlockDirection::from_id((id >> 1) & 0b11), (id & 1) == 0)
        },
        from_names(name): {
            "stone_button" => {
                button: Default::default()
            }
        },
    },
    Sign {
        props: {
            sign_type: u32,
            rotation: u32
        },
        get_id: (sign_type << 5) + (rotation << 1) + 3382,
        from_id(id): 3381..=3571 => {
            sign_type: id >> 5,
            rotation: (id & 0b11110) >> 1
        },
        from_names(name): {
            "oak_sign" => {
                sign_type: 0,
                rotation: 0
            },
            "spruce_sign" => {
                sign_type: 1,
                rotation: 0
            },
            "birch_sign" => {
                sign_type: 2,
                rotation: 0
            },
            "jungle_sign" => {
                sign_type: 3,
                rotation: 0
            },
            "acacia_sign" => {
                sign_type: 4,
                rotation: 0
            },
            "dark_oak_sign" => {
                sign_type: 5,
                rotation: 0
            }
        },
    },
    RedstoneTorch {
        props: {
            lit: bool
        },
        get_id: if lit {
            3887
        } else {
            3888
        },
        from_id_offset: 3887,
        from_id(id): 3887..=3888 => {
            lit: id == 0
        },
        from_names(name): {
            "redstone_torch" => {
                lit: true
            }
        },
    },
    RedstoneWallTorch {
        props: {
            lit: bool,
            facing: BlockDirection
        },
        get_id: (facing.get_id() << 1) + (!lit as u32) + 3889,
        from_id_offset: 3889,
        from_id(id): 3889..=3896 => {
            lit: (id & 1) == 0,
            facing: BlockDirection::from_id(id >> 1)
        },
        from_names(name): {
            "redstone_wall_torch" => {
                lit: true,
                facing: Default::default()
            }
        },
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
                + 4031
        },
        from_id_offset: 4031,
        from_id(id): 4031..=4094 => {
            repeater: RedstoneRepeater::new(
                (id >> 4) as u8 + 1,
                BlockDirection::from_id((id >> 2) & 3),
                ((id >> 1) & 1) == 0,
                (id & 1) == 0
            )
        },
        from_names(name): {
            "repeater" => {
                repeater: Default::default()
            }
        },
    },
    RedstoneLamp {
        props: {
            lit: bool
        },
        get_id: if lit {
            5160
        } else {
            5161
        },
        from_id_offset: 5160,
        from_id(id): 5160..=5161 => {
            lit: id == 0
        },
        from_names(name): {
            "redstone_lamp" => {
                lit: false
            }
        },
    },
    TripwireHook {
        props: {
            direction: BlockDirection
        },
        get_id: match direction {
            BlockDirection::North => 5272,
            BlockDirection::South => 5274,
            BlockDirection::West => 5276,
            BlockDirection::East => 5278,
        },
        from_id_offset: 5272,
        from_id(id): 5272..=5278 => {
            direction: BlockDirection::from_id(id / 2)
        },
        from_names(name): {
            "tripwire_hook" => {
                direction: Default::default()
            }
        },
    },
    RedstoneComparator {
        props: {
            comparator: RedstoneComparator
        },
        get_id: {
            comparator.facing.get_id() * 4
                + comparator.mode.get_id() * 2
                + !comparator.powered as u32
                + 6682
        },
        from_id_offset: 6682,
        from_id(id): 6682..=6697 => {
            comparator: RedstoneComparator::new(
                BlockDirection::from_id(id >> 2),
                ComparatorMode::from_id((id >> 1) & 1),
                (id & 1) == 0
            )
        },
        from_names(name): {
            "comparator" => {
                comparator: Default::default()
            }
        },
    },
    RedstoneBlock {
        props: {},
        get_id: 6730,
        from_id(id): 6730 => {},
        from_names(name): {
            "redstone_block" => {}
        },
        transparent: true,
        cube: true,
    },
    Observer {
        props: {
            facing: BlockFacing
        },
        get_id: (facing.get_id() << 1) + 9265,
        from_id_offset: 9265,
        from_id(id): 9265..=9275 => {
            facing: BlockFacing::from_id(id >> 1)
        },
        from_names(name): {
            "observer" => {
                facing: Default::default()
            }
        },
        solid: true,
        cube: true,
    },
    SeaPickle {
        props: {
            pickles: u8
        },
        get_id: ((pickles - 1) << 1) as u32 + 9645,
        from_id_offset: 9645,
        from_id(id): 0 => {
            pickles: (id >> 1) as u8 + 1
        },
        from_names(name): {
            "sea_pickle" => {
                pickles: 1
            }
        },
    },
    Target {
        props: {},
        get_id: 15768,
        from_id(id): 15768 => {},
        from_names(name): {
            "target" => {}
        },
        solid: true,
        cube: true,
    },
    StonePressurePlate {
        props: {},
        get_id: 3808,
        from_id(id): 3808 => {},
        from_names(name): {
            "stone_pressure_plate" => {}
        },
    },
    Barrel {
        props: {},
        get_id: 14796,
        from_id(id): 14796 => {},
        from_names(name): {
            "barrel" => {}
        },
        solid: true,
        cube: true,
    },
    Hopper {
        props: {},
        get_id: 6738,
        from_id(id): 6738 => {},
        from_names(name): {
            "hopper" => {}
        },
        transparent: true,
        cube: true,
    },
    Sandstone {
        props: {},
        get_id: 246,
        from_id(id): 246 => {},
        from_names(name): {
            "sandstone" => {}
        },
        solid: true,
        cube: true,
    },
    Furnace {
        props: {},
        get_id: 3374,
        from_id(id): 3374 => {},
        from_names(name): {
            "furnace" => {}
        },
        solid: true,
        cube: true,
    },
    SmoothStoneSlab {
        props: {},
        get_id: 8347,
        from_id(id): 8347 => {},
        from_names(name): {
            "smooth_stone_slab" => {}
        },
        transparent: true,
        cube: true,
    },
    QuartzSlab {
        props: {},
        get_id: 8395,
        from_id(id): 8395 => {},
        from_names(name): {
            "quartz_slab" => {}
        },
        transparent: true,
        cube: true,
    },
    Concrete {
        props: {
            color: BlockColorVariant
        },
        get_id: color.get_id() + 9442,
        from_id_offset: 9442,
        from_id(id): 9442..=9457 => {
            color: BlockColorVariant::from_id(id)
        },
        from_names(name): {
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
        solid: true,
        cube: true,
    },
    StainedGlass {
        props: {
            color: BlockColorVariant
        },
        get_id: color.get_id() + 4095,
        from_id_offset: 4095,
        from_id(id): 4095..=4110 => {
            color: BlockColorVariant::from_id(id)
        },
        from_names(name): {
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
        transparent: true,
        cube: true,
    },
    Terracotta {
        props: {
            color: BlockColorVariant
        },
        get_id: color.get_id() + 6851,
        from_id_offset: 6851,
        from_id(id): 6851..=6866 => {
            color: BlockColorVariant::from_id(id)
        },
        from_names(name): {
            "white_terracotta" => { color: BlockColorVariant::White },
            "orange_terracotta" => { color: BlockColorVariant::Orange },
            "magenta_terracotta" => { color: BlockColorVariant::Magenta },
            "light_blue_terracotta" => { color: BlockColorVariant::LightBlue },
            "yellow_terracotta" => { color: BlockColorVariant::Yellow },
            "lime_terracotta" => { color: BlockColorVariant::Lime },
            "pink_terracotta" => { color: BlockColorVariant::Pink },
            "gray_terracotta" => { color: BlockColorVariant::Gray },
            "light_terracotta" => { color: BlockColorVariant::LightGray },
            "cyan_terracotta" => { color: BlockColorVariant::Cyan },
            "purple_terracotta" => { color: BlockColorVariant::Purple },
            "blue_terracotta" => { color: BlockColorVariant::Blue },
            "brown_terracotta" => { color: BlockColorVariant::Brown },
            "green_terracotta" => { color: BlockColorVariant::Green },
            "red_terracotta" => { color: BlockColorVariant::Red },
            "black_terracotta" => { color: BlockColorVariant::Black }
        },
        solid: true,
        cube: true,
    },
    Wool {
        props: {
            color: BlockColorVariant
        },
        get_id: color.get_id() + 1384,
        from_id_offset: 1384,
        from_id(id): 1384..=1399 => {
            color: BlockColorVariant::from_id(id)
        },
        from_names(name): {
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
        solid: true,
        cube: true,
    }
}
