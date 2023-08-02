use crate::config::CONFIG;
use crate::player::Player;
use crate::plot::PlotWorld;
use crate::plot::PLOT_BLOCK_HEIGHT;
use crate::redstone;
use crate::world::World;
use mchprs_blocks::block_entities::BlockEntity;
use mchprs_blocks::blocks::*;
use mchprs_blocks::items::{Item, ItemStack};
use mchprs_blocks::{BlockFace, BlockPos, SignType};
use mchprs_network::packets::clientbound::{COpenSignEditor, ClientBoundPacket};
use mchprs_world::TickPriority;

pub fn on_use(
    block: Block,
    world: &mut impl World,
    player: &mut Player,
    pos: BlockPos,
    item_in_hand: Option<Item>,
) -> ActionResult {
    match block {
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
            redstone::comparator::tick(comparator, world, pos);
            world.set_block(pos, Block::RedstoneComparator { comparator });
            ActionResult::Success
        }
        Block::Lever { mut lever } => {
            lever.powered = !lever.powered;
            world.set_block(pos, Block::Lever { lever });
            redstone::update_surrounding_blocks(world, pos);
            match lever.face {
                LeverFace::Ceiling => {
                    redstone::update_surrounding_blocks(world, pos.offset(BlockFace::Top));
                }
                LeverFace::Floor => {
                    redstone::update_surrounding_blocks(world, pos.offset(BlockFace::Bottom));
                }
                LeverFace::Wall => redstone::update_surrounding_blocks(
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
                redstone::update_surrounding_blocks(world, pos);
                match button.face {
                    ButtonFace::Ceiling => {
                        redstone::update_surrounding_blocks(world, pos.offset(BlockFace::Top));
                    }
                    ButtonFace::Floor => {
                        redstone::update_surrounding_blocks(world, pos.offset(BlockFace::Bottom));
                    }
                    ButtonFace::Wall => redstone::update_surrounding_blocks(
                        world,
                        pos.offset(button.facing.opposite().block_face()),
                    ),
                }
            }
            ActionResult::Success
        }
        Block::RedstoneWire { wire } => redstone::wire::on_use(wire, world, pos),
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

pub fn get_state_for_placement(
    world: &impl World,
    pos: BlockPos,
    item: Item,
    context: &UseOnBlockContext<'_>,
) -> Block {
    let block = match item {
        Item::Stone {} => Block::Stone {},
        Item::Glass {} => Block::Glass {},
        Item::Sandstone {} => Block::Sandstone {},
        Item::SeaPickle {} => Block::SeaPickle { pickles: 1 },
        Item::Wool { color } => Block::Wool { color },
        Item::Furnace {} => Block::Furnace {},
        Item::StonePressurePlate {} => Block::StonePressurePlate { powered: false },
        Item::Lever {} => {
            let lever_face = match context.block_face {
                BlockFace::Top => LeverFace::Floor,
                BlockFace::Bottom => LeverFace::Ceiling,
                _ => LeverFace::Wall,
            };
            let facing = if lever_face == LeverFace::Wall {
                context.block_face.to_direction()
            } else {
                context.player.get_direction()
            };
            Block::Lever {
                lever: Lever::new(lever_face, facing, false),
            }
        }
        Item::RedstoneTorch {} => match context.block_face {
            BlockFace::Top | BlockFace::Bottom => Block::RedstoneTorch { lit: true },
            face => Block::RedstoneWallTorch {
                lit: true,
                facing: face.to_direction(),
            },
        },
        Item::TripwireHook {} => match context.block_face {
            BlockFace::Bottom | BlockFace::Top => Block::Air {},
            direction => Block::TripwireHook {
                direction: direction.to_direction(),
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
                context.player.get_direction()
            };
            Block::StoneButton {
                button: StoneButton::new(button_face, facing, false),
            }
        }
        Item::RedstoneLamp {} => Block::RedstoneLamp {
            lit: redstone::redstone_lamp_should_be_lit(world, pos),
        },
        Item::RedstoneBlock {} => Block::RedstoneBlock {},
        Item::Hopper {} => Block::Hopper {},
        Item::Terracotta {} => Block::Terracotta {},
        Item::ColoredTerracotta { color } => Block::ColoredTerracotta { color },
        Item::Concrete { color } => Block::Concrete { color },
        Item::Repeater {} => Block::RedstoneRepeater {
            repeater: redstone::repeater::get_state_for_placement(
                world,
                pos,
                context.player.get_direction().opposite(),
            ),
        },
        Item::Comparator {} => Block::RedstoneComparator {
            comparator: RedstoneComparator::new(
                context.player.get_direction().opposite(),
                ComparatorMode::Compare,
                false,
            ),
        },
        Item::Sign { sign_type } => match context.block_face {
            BlockFace::Bottom => Block::Air {},
            BlockFace::Top => Block::Sign {
                sign_type: SignType(sign_type),
                rotation: (((180.0 + context.player.yaw) * 16.0 / 360.0) + 0.5).floor() as u32 & 15,
            },
            _ => Block::WallSign {
                sign_type: SignType(sign_type),
                facing: context.block_face.to_direction(),
            },
        },
        Item::Redstone {} => Block::RedstoneWire {
            wire: redstone::wire::get_state_for_placement(world, pos),
        },
        Item::Barrel {} => Block::Barrel {},
        Item::Target {} => Block::Target {},
        Item::StainedGlass { color } => Block::StainedGlass { color },
        Item::SmoothStoneSlab {} => Block::SmoothStoneSlab {},
        Item::QuartzSlab {} => Block::QuartzSlab {},
        Item::IronTrapdoor {} => match context.block_face {
            BlockFace::Bottom => Block::IronTrapdoor {
                facing: context.player.get_direction().opposite(),
                half: TrapdoorHalf::Top,
                powered: false,
            },
            BlockFace::Top => Block::IronTrapdoor {
                facing: context.player.get_direction().opposite(),
                half: TrapdoorHalf::Bottom,
                powered: false,
            },
            _ => Block::IronTrapdoor {
                facing: context.block_face.to_direction(),
                half: if context.cursor_y > 0.5 {
                    TrapdoorHalf::Top
                } else {
                    TrapdoorHalf::Bottom
                },
                powered: false,
            },
        },
        _ => Block::Air {},
    };
    if is_valid_position(block, world, pos) {
        block
    } else {
        Block::Air {}
    }
}

pub fn place_in_world(
    block: Block,
    world: &mut impl World,
    pos: BlockPos,
    nbt: &Option<nbt::Blob>,
) {
    if block.has_block_entity() {
        if let Some(nbt) = nbt {
            if let nbt::Value::Compound(compound) = &nbt["BlockEntityTag"] {
                if let Some(block_entity) = BlockEntity::from_nbt(compound) {
                    world.set_block_entity(pos, block_entity);
                }
            }
        };
    }
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
            world.set_block(pos, Block::Air {});
            change_surrounding_blocks(world, pos);
            redstone::update_wire_neighbors(world, pos);
        }
        Block::Lever { lever } => {
            world.set_block(pos, Block::Air {});
            // This is a horrible idea, don't do this.
            // One day this will be fixed, but for now... too bad!
            match lever.face {
                LeverFace::Ceiling => {
                    change_surrounding_blocks(world, pos.offset(BlockFace::Top));
                    redstone::update_surrounding_blocks(world, pos.offset(BlockFace::Top));
                }
                LeverFace::Floor => {
                    change_surrounding_blocks(world, pos.offset(BlockFace::Bottom));
                    redstone::update_surrounding_blocks(world, pos.offset(BlockFace::Bottom));
                }
                LeverFace::Wall => {
                    change_surrounding_blocks(
                        world,
                        pos.offset(lever.facing.opposite().block_face()),
                    );
                    redstone::update_surrounding_blocks(
                        world,
                        pos.offset(lever.facing.opposite().block_face()),
                    );
                }
            }
        }
        _ => {
            world.set_block(pos, Block::Air {});
            change_surrounding_blocks(world, pos);
            redstone::update_surrounding_blocks(world, pos);
        }
    }
}

pub fn is_valid_position(block: Block, world: &impl World, pos: BlockPos) -> bool {
    if world.is_cursed() {
        return true;
    }

    match block {
        Block::RedstoneWire { .. }
        | Block::RedstoneComparator { .. }
        | Block::RedstoneRepeater { .. }
        | Block::Sign { .. }
        | Block::RedstoneTorch { .. } => {
            let bottom_block = world.get_block(pos.offset(BlockFace::Bottom));
            bottom_block.is_cube()
        }
        Block::RedstoneWallTorch { facing, .. } | Block::WallSign { facing, .. } => {
            let parent_block = world.get_block(pos.offset(facing.opposite().block_face()));
            parent_block.is_cube()
        }
        Block::TripwireHook { direction, .. } => {
            let parent_block = world.get_block(pos.offset(direction.opposite().block_face()));
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

pub fn change(block: Block, world: &mut impl World, pos: BlockPos, direction: BlockFace) {
    if !is_valid_position(block, world, pos) {
        destroy(block, world, pos);
        return;
    }
    if let Block::RedstoneWire { wire } = block {
        let new_state = redstone::wire::on_neighbor_changed(wire, world, pos, direction);
        if world.set_block(pos, Block::RedstoneWire { wire: new_state }) {
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

        match block {
            Block::Sign { .. } | Block::WallSign { .. } => {
                let open_sign_editor = COpenSignEditor {
                    pos_x: block_pos.x,
                    pos_y: block_pos.y,
                    pos_z: block_pos.z,
                }
                .encode();
                ctx.player.client.send_packet(&open_sign_editor);
            }
            _ => {}
        }

        place_in_world(block, world, block_pos, &item.nbt);
        false
    } else {
        true
    }
}
