use crate::config::CONFIG;
use crate::player::Player;
use crate::plot::{PlotWorld, PLOT_BLOCK_HEIGHT};
use mchprs_blocks::block_entities::BlockEntity;
use mchprs_blocks::items::{Item, ItemStack};
use mchprs_blocks::{blocks::*, BlockDirection, BlockFacing};
use mchprs_blocks::{BlockFace, BlockPos};
use mchprs_network::packets::clientbound::{COpenSignEditor, ClientBoundPacket};
use mchprs_redstone as redstone;
use mchprs_utils::nbt_unwrap_val;
use mchprs_world::World;

pub fn on_use(
    block: Block,
    world: &mut impl World,
    player: &mut Player,
    pos: BlockPos,
    item_in_hand: Option<Item>,
) -> ActionResult {
    if redstone::on_use(block, world, pos) {
        return ActionResult::Success;
    }

    match block {
        Block::SeaPickle {
            pickles,
            waterlogged,
        } => {
            if let Some(Item::SeaPickle) = item_in_hand
                && pickles < 4
            {
                world.set_block(
                    pos,
                    Block::SeaPickle {
                        pickles: pickles + 1,
                        waterlogged,
                    },
                );
            }
            ActionResult::Success
        }
        Block::EndPortalFrame { eye, facing } => {
            if let Some(Item::EnderEye) = item_in_hand
                && !eye
            {
                world.set_block(pos, Block::EndPortalFrame { eye: true, facing });
                redstone::update_surrounding_blocks(world, pos);
                return ActionResult::Success;
            }
            ActionResult::Pass
        }
        b if b.has_block_entity() => {
            // Open container
            let block_entity = world.get_block_entity(pos);
            if let Some(BlockEntity::Container { inventory, ty, .. }) = block_entity {
                player.open_container(inventory, *ty);
            }
            ActionResult::Success
        }
        _ => ActionResult::Pass,
    }
}

fn get_sign_placement(
    context: &UseOnBlockContext<'_>,
    standard: fn(u8) -> Block,
    wall: fn(BlockDirection) -> Block,
) -> Block {
    let rotation = (((180.0 + context.player.yaw) * 16.0 / 360.0) + 0.5).floor() as u8 & 15;

    match context.block_face {
        BlockFace::Bottom => Block::Air,
        BlockFace::Top => standard(rotation),
        _ => wall(context.block_face.unwrap_direction()),
    }
}

pub fn get_state_for_placement(
    world: &impl World,
    pos: BlockPos,
    item: Item,
    context: &UseOnBlockContext<'_>,
) -> Block {
    let simple_block = item.get_simple_placement();
    macro_rules! sign_placement {
        ($standard_block:ident, $wall_block:ident) => {
            get_sign_placement(
                context,
                |rotation| Block::$standard_block {
                    waterlogged: false,
                    rotation,
                },
                |facing| Block::$wall_block {
                    waterlogged: false,
                    facing,
                },
            )
        };
    }
    let block = match item {
        Item::OakSign => sign_placement!(OakSign, OakWallSign),
        Item::SpruceSign => sign_placement!(SpruceSign, SpruceWallSign),
        Item::BirchSign => sign_placement!(BirchSign, BirchWallSign),
        Item::AcaciaSign => sign_placement!(AcaciaSign, AcaciaWallSign),
        Item::JungleSign => sign_placement!(JungleSign, JungleWallSign),
        Item::DarkOakSign => sign_placement!(DarkOakSign, DarkOakWallSign),
        Item::CrimsonSign => sign_placement!(CrimsonSign, CrimsonWallSign),
        Item::WarpedSign => sign_placement!(WarpedSign, WarpedWallSign),
        Item::BambooSign => sign_placement!(BambooSign, BambooWallSign),
        Item::CherrySign => sign_placement!(CherrySign, CherryWallSign),
        Item::MangroveSign => sign_placement!(MangroveSign, MangroveWallSign),
        Item::SeaPickle => Block::SeaPickle {
            pickles: 1,
            waterlogged: false,
        },
        Item::Furnace => Block::Furnace {
            facing: context.player.get_direction().opposite(),
            lit: false,
        },
        Item::OakPressurePlate => Block::OakPressurePlate { powered: false },
        Item::SprucePressurePlate => Block::SprucePressurePlate { powered: false },
        Item::BirchPressurePlate => Block::BirchPressurePlate { powered: false },
        Item::JunglePressurePlate => Block::JunglePressurePlate { powered: false },
        Item::AcaciaPressurePlate => Block::AcaciaPressurePlate { powered: false },
        Item::DarkOakPressurePlate => Block::DarkOakPressurePlate { powered: false },
        Item::MangrovePressurePlate => Block::MangrovePressurePlate { powered: false },
        Item::CherryPressurePlate => Block::CherryPressurePlate { powered: false },
        Item::BambooPressurePlate => Block::BambooPressurePlate { powered: false },
        Item::CrimsonPressurePlate => Block::CrimsonPressurePlate { powered: false },
        Item::WarpedPressurePlate => Block::WarpedPressurePlate { powered: false },
        Item::StonePressurePlate => Block::StonePressurePlate { powered: false },
        Item::PolishedBlackstonePressurePlate => {
            Block::PolishedBlackstonePressurePlate { powered: false }
        }
        Item::Lever => {
            let face = match context.block_face {
                BlockFace::Top => LeverFace::Floor,
                BlockFace::Bottom => LeverFace::Ceiling,
                _ => LeverFace::Wall,
            };
            let facing = if face == LeverFace::Wall {
                context.block_face.unwrap_direction()
            } else {
                context.player.get_direction()
            };
            Block::Lever {
                face,
                facing,
                powered: false,
            }
        }
        Item::RedstoneTorch => match context.block_face {
            BlockFace::Top | BlockFace::Bottom => Block::RedstoneTorch { lit: true },
            face => Block::RedstoneWallTorch {
                lit: true,
                facing: face.unwrap_direction(),
            },
        },
        Item::TripwireHook => match context.block_face {
            BlockFace::Bottom | BlockFace::Top => Block::Air,
            direction => Block::TripwireHook {
                facing: direction.unwrap_direction(),
                powered: false,
                attached: false,
            },
        },
        Item::StoneButton => {
            let face = match context.block_face {
                BlockFace::Top => LeverFace::Floor,
                BlockFace::Bottom => LeverFace::Ceiling,
                _ => LeverFace::Wall,
            };
            let facing = if face == LeverFace::Wall {
                context.block_face.unwrap_direction()
            } else {
                context.player.get_direction()
            };
            Block::StoneButton {
                face,
                facing,
                powered: false,
            }
        }
        Item::RedstoneLamp => Block::RedstoneLamp {
            lit: redstone::redstone_lamp_should_be_lit(world, pos),
        },
        // TODO: Hopper facing
        Item::Hopper => Block::Hopper {
            enabled: false,
            facing: HopperFacing::Down,
        },
        Item::Repeater => Block::Repeater(redstone::repeater::get_state_for_placement(
            world,
            pos,
            context.player.get_direction().opposite(),
        )),
        Item::Comparator => Block::Comparator(Comparator::new(
            context.player.get_direction().opposite(),
            ComparatorMode::Compare,
            false,
        )),
        Item::Redstone => Block::RedstoneWire(redstone::wire::get_state_for_placement(world, pos)),
        // TODO: Barrel facing
        Item::Barrel => Block::Barrel {
            facing: BlockFacing::Up,
            open: false,
        },
        Item::Target => Block::Target { power: 0 },
        Item::SmoothStoneSlab => Block::SmoothStoneSlab {
            ty: SlabType::Top,
            waterlogged: false,
        },
        Item::QuartzSlab => Block::QuartzSlab {
            ty: SlabType::Top,
            waterlogged: false,
        },
        Item::IronTrapdoor => match context.block_face {
            BlockFace::Bottom => Block::IronTrapdoor {
                facing: context.player.get_direction().opposite(),
                half: TrapdoorHalf::Top,
                powered: false,
                open: false,
                waterlogged: false,
            },
            BlockFace::Top => Block::IronTrapdoor {
                facing: context.player.get_direction().opposite(),
                half: TrapdoorHalf::Bottom,
                open: false,
                waterlogged: false,
                powered: false,
            },
            _ => Block::IronTrapdoor {
                facing: context.block_face.unwrap_direction(),
                half: if context.cursor_y > 0.5 {
                    TrapdoorHalf::Top
                } else {
                    TrapdoorHalf::Bottom
                },
                open: false,
                waterlogged: false,
                powered: false,
            },
        },
        Item::NoteBlock => Block::NoteBlock {
            instrument: Instrument::Harp,
            note: 0,
            powered: false,
        },
        Item::BoneBlock => Block::BoneBlock { axis: BlockAxis::Y },
        Item::HayBlock => Block::HayBlock { axis: BlockAxis::Y },
        Item::EndPortalFrame => Block::EndPortalFrame {
            eye: false,
            facing: context.player.get_direction().opposite(),
        },
        _ => Block::Air,
    };
    let block = simple_block.unwrap_or(block);
    if is_valid_position(block, world, pos) {
        block
    } else {
        Block::Air
    }
}

fn read_block_entity_tag(nbt: &nbt::Blob, block_id: &str) -> Option<BlockEntity> {
    if let nbt::Value::Compound(compound) = &nbt["BlockEntityTag"] {
        let id = match nbt.get("Id").or_else(|| nbt.get("id")) {
            Some(id) => nbt_unwrap_val!(id, nbt::Value::String),
            None => block_id,
        };
        return BlockEntity::from_nbt(id, compound);
    }

    None
}

pub fn place_in_world(
    block: Block,
    world: &mut impl World,
    pos: BlockPos,
    nbt: &Option<nbt::Blob>,
) {
    if block.has_block_entity()
        && let Some(nbt) = nbt
        && let Some(block_entity) = read_block_entity_tag(nbt, block.get_name())
    {
        world.set_block_entity(pos, block_entity);
    };
    world.set_block(pos, block);
    change_surrounding_blocks(world, pos);
    if let Block::RedstoneWire { .. } = block {
        redstone::update_wire_neighbors(world, pos);
    } else {
        redstone::update_surrounding_blocks(world, pos);
    }
}

pub fn destroy(block: Block, world: &mut impl World, pos: BlockPos) {
    if block.has_block_entity() {
        world.delete_block_entity(pos);
    }

    match block {
        Block::RedstoneWire { .. } => {
            world.set_block(pos, Block::Air);
            change_surrounding_blocks(world, pos);
            redstone::update_wire_neighbors(world, pos);
        }
        Block::Lever { face, facing, .. } => {
            world.set_block(pos, Block::Air);
            // This is a horrible idea, don't do this.
            // One day this will be fixed, but for now... too bad!
            match face {
                LeverFace::Ceiling => {
                    change_surrounding_blocks(world, pos.offset(BlockFace::Top));
                    redstone::update_surrounding_blocks(world, pos.offset(BlockFace::Top));
                }
                LeverFace::Floor => {
                    change_surrounding_blocks(world, pos.offset(BlockFace::Bottom));
                    redstone::update_surrounding_blocks(world, pos.offset(BlockFace::Bottom));
                }
                LeverFace::Wall => {
                    change_surrounding_blocks(world, pos.offset(facing.opposite().block_face()));
                    redstone::update_surrounding_blocks(
                        world,
                        pos.offset(facing.opposite().block_face()),
                    );
                }
            }
        }
        _ => {
            world.set_block(pos, Block::Air);
            change_surrounding_blocks(world, pos);
            redstone::update_surrounding_blocks(world, pos);
        }
    }
}

pub fn is_valid_position(block: Block, world: &impl World, pos: BlockPos) -> bool {
    if world.is_cursed() {
        return true;
    }

    let check_bottom = matches!(
        block,
        Block::RedstoneWire(_)
            | Block::Comparator(_)
            | Block::Repeater(_)
            | Block::RedstoneTorch { .. }
            | Block::Lever {
                face: LeverFace::Floor,
                ..
            }
            | Block::StoneButton {
                face: LeverFace::Floor,
                ..
            }
    ) || block.is_sign();

    let check_top = matches!(
        block,
        Block::Lever {
            face: LeverFace::Ceiling,
            ..
        } | Block::StoneButton {
            face: LeverFace::Ceiling,
            ..
        }
    );

    let check_parent = block.get_wall_sign_facing().or(match block {
        Block::TripwireHook { facing, .. } => Some(facing),
        Block::StoneButton {
            face: LeverFace::Wall,
            facing,
            ..
        } => Some(facing),
        Block::Lever {
            face: LeverFace::Wall,
            facing,
            ..
        } => Some(facing),
        _ => None,
    });

    if check_bottom {
        let bottom_block = world.get_block(pos.offset(BlockFace::Bottom));
        bottom_block.is_cube()
    } else if check_top {
        let top_block = world.get_block(pos.offset(BlockFace::Top));
        top_block.is_cube()
    } else if let Some(facing) = check_parent {
        let parent_block = world.get_block(pos.offset(facing.opposite().block_face()));
        parent_block.is_cube()
    } else {
        true
    }
}

pub fn change(block: Block, world: &mut impl World, pos: BlockPos, direction: BlockFace) {
    if !is_valid_position(block, world, pos) {
        destroy(block, world, pos);
        return;
    }
    if let Block::RedstoneWire(wire) = block {
        let new_state = redstone::wire::on_neighbor_changed(wire, world, pos, direction);
        if world.set_block(pos, Block::RedstoneWire(new_state)) {
            redstone::update_wire_neighbors(world, pos);
        }
    }
}

fn change_surrounding_blocks(world: &mut impl World, pos: BlockPos) {
    for direction in &BlockFace::values() {
        let neighbor_pos = pos.offset(*direction);
        let block = world.get_block(neighbor_pos);
        change(block, world, neighbor_pos, *direction);

        // Also change diagonal blocks

        let up_pos = neighbor_pos.offset(BlockFace::Top);
        let up_block = world.get_block(up_pos);
        change(up_block, world, up_pos, *direction);

        let down_pos = neighbor_pos.offset(BlockFace::Bottom);
        let down_block = world.get_block(down_pos);
        change(down_block, world, down_pos, *direction);
    }
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub enum ActionResult {
    Success,
    Pass,
}

impl ActionResult {
    fn is_success(self) -> bool {
        self == ActionResult::Success
    }
}

pub struct UseOnBlockContext<'a> {
    pub block_pos: BlockPos,
    pub block_face: BlockFace,
    pub player: &'a mut Player,
    pub cursor_y: f32,
}

/// returns true if cancelled
pub fn use_item_on_block(
    item: &ItemStack,
    world: &mut PlotWorld,
    ctx: UseOnBlockContext<'_>,
) -> bool {
    let use_pos = ctx.block_pos;
    let use_block = world.get_block(use_pos);
    let block_pos = ctx.block_pos.offset(ctx.block_face);
    let mut top_pos = ctx.player.pos.block_pos();
    top_pos.y += 1;
    if (block_pos == ctx.player.pos.block_pos() || block_pos == top_pos) && !CONFIG.block_in_hitbox
    {
        return false;
    }
    let can_place = item.item_type.is_block() && world.get_block(block_pos).can_place_block_in();

    if !ctx.player.crouching
        && on_use(
            use_block,
            world,
            ctx.player,
            ctx.block_pos,
            Some(item.item_type),
        )
        .is_success()
    {
        return false;
    }

    if can_place && (0..PLOT_BLOCK_HEIGHT).contains(&block_pos.y) {
        let block = get_state_for_placement(world, block_pos, item.item_type, &ctx);

        if (block.is_sign() || block.is_wall_sign())
            && !item
                .nbt
                .as_ref()
                .is_some_and(|blob| blob.content.contains_key("BlockEntityTag"))
        {
            let open_sign_editor = COpenSignEditor {
                pos_x: block_pos.x,
                pos_y: block_pos.y,
                pos_z: block_pos.z,
                // TODO: editing back text
                is_front_text: true,
            }
            .encode();
            ctx.player.client.send_packet(&open_sign_editor);
        }

        place_in_world(block, world, block_pos, &item.nbt);
        false
    } else {
        true
    }
}
