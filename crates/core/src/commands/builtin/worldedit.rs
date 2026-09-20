use crate::{
    commands::{
        argument::ArgumentType,
        context::ExecutionContext,
        define::{arg, opt, Command},
        error::{CommandError, CommandResult, RuntimeError},
        registry::CommandRegistry,
        schematic::{schematic_names, schematic_path},
        value::{Direction, DirectionWithDiagonals, ReplacementOperand},
    },
    player::PlayerPos,
    plot::{PlotWorld, PLOT_BLOCK_HEIGHT},
    utils,
    worldedit::{
        calculate_expanded_selection, calculate_selection_volume, clear_area, create_clipboard,
        mask::WorldEditMask, paste_clipboard, pattern::WorldEditPattern, region_positions, update,
        WorldEditClipboard, WorldEditUndo,
    },
};
use itertools::Itertools;
use mchprs_blocks::{
    block_entities::{BlockEntity, ContainerType, InventoryEntry},
    blocks::{Block, FlipDirection, HopperFacing, RotateAmt},
    items::{Item, ItemStack},
    BlockDirection, BlockFacing, BlockPos,
};
use mchprs_network::packets::clientbound::*;
use mchprs_schematic::{load_schematic, save_schematic};
use mchprs_world::{storage::PalettedBitBuffer, World};
use rustc_hash::FxHashMap;
use std::{io, time::Instant};
use tracing::warn;

pub(super) fn register_commands(registry: &mut CommandRegistry) {
    register_navigation(registry);
    register_selection(registry);
    register_region(registry);
    register_clipboard(registry);
    register_history(registry);
    register_schematic(registry);
}

fn register_navigation(registry: &mut CommandRegistry) {
    registry.register(Command::new("jumpto").alias("j").executes(|ctx| {
        let pos = ctx.target_block(1000.0)?;
        let new_pos = PlayerPos::new(pos.x as f64 + 0.5, pos.y as f64 + 1.0, pos.z as f64 + 0.5);
        ctx.player_mut()?.teleport(new_pos);
        ctx.worldedit_message("Teleported to block.")
    }));

    registry.register(Command::new("unstuck").alias("!").executes(|ctx| {
        let player_pos = ctx.player()?.pos.block_pos();

        let world = ctx.world();
        let found_y = (player_pos.y..PLOT_BLOCK_HEIGHT - 1).find(|&y| {
            let pos = BlockPos::new(player_pos.x, y, player_pos.z);
            let head = BlockPos::new(player_pos.x, y + 1, player_pos.z);
            world.get_block(pos) == Block::Air && world.get_block(head) == Block::Air
        });

        let y = found_y.ok_or_else(|| CommandError::runtime("No free spot above found."))?;
        let new_pos = PlayerPos::new(
            player_pos.x as f64 + 0.5,
            y as f64,
            player_pos.z as f64 + 0.5,
        );

        ctx.player_mut()?.teleport(new_pos);
        ctx.worldedit_message("Moved to free position.")
    }));

    registry.register(
        Command::new("up")
            .alias("u")
            .permission("worldedit.navigation.up")
            .require_plot_ownership()
            .optional("distance", ArgumentType::integer(1, 100))
            .flag('f', "force-flight", "Force using flight to keep you still")
            .flag('g', "force-glass", "Force using glass to keep you still")
            .executes(|ctx| {
                let distance = ctx.arg_or("distance", 1)?;
                let force_flight = ctx.flag("force-flight");
                let force_glass = ctx.flag("force-glass");

                let player = ctx.player_mut()?;
                let mut new_pos = player.pos;
                new_pos.y += distance as f64;

                if force_flight {
                    player.flying = true;
                    player.update_player_abilities();
                }

                if force_glass || !force_flight {
                    let block_pos = new_pos.block_pos();
                    let platform_pos = BlockPos::new(block_pos.x, block_pos.y - 1, block_pos.z);
                    if matches!(ctx.world().get_block(platform_pos), Block::Air) {
                        ctx.world_mut().set_block(platform_pos, Block::Glass);
                    }
                }

                ctx.player_mut()?.teleport(new_pos);
                ctx.worldedit_message(&format!("Moved up {} blocks", distance))
            }),
    );

    fn exec_floors(
        ctx: &mut ExecutionContext<'_>,
        ys: impl Iterator<Item = i32>,
        verb: &str,
        error: &str,
    ) -> CommandResult<()> {
        let levels: i32 = ctx.arg_or("levels", 1)?;
        let player_pos = ctx.player()?.pos.block_pos();
        let world = ctx.world();

        let mut player_y = player_pos.y;
        let mut remaining = levels;
        for y in ys {
            if remaining == 0 {
                break;
            }
            let floor_pos = BlockPos::new(player_pos.x, y - 1, player_pos.z);
            let pos = BlockPos::new(player_pos.x, y, player_pos.z);
            let high_pos = BlockPos::new(player_pos.x, y + 1, player_pos.z);

            if world.get_block(floor_pos) != Block::Air
                && world.get_block(pos) == Block::Air
                && world.get_block(high_pos) == Block::Air
            {
                player_y = y;
                remaining -= 1;
            }
        }

        if player_y == player_pos.y {
            return Err(CommandError::runtime(error));
        }

        let mut new_pos = ctx.player()?.pos;
        new_pos.y = player_y as f64;
        ctx.player_mut()?.teleport(new_pos);
        ctx.worldedit_message(&format!("{} {} levels", verb, levels - remaining))
    }

    registry.register(
        Command::new("ascend")
            .alias("asc")
            .permission("worldedit.navigation.ascend")
            .require_plot_ownership()
            .optional("levels", ArgumentType::integer(1, 100))
            .executes(|ctx| {
                let y = ctx.player()?.pos.block_pos().y;
                exec_floors(
                    ctx,
                    y + 1..=PLOT_BLOCK_HEIGHT,
                    "Ascended",
                    "No free spot above you found.",
                )
            }),
    );

    registry.register(
        Command::new("descend")
            .alias("desc")
            .permission("worldedit.navigation.descend")
            .require_plot_ownership()
            .optional("levels", ArgumentType::integer(1, 100))
            .executes(|ctx| {
                let y = ctx.player()?.pos.block_pos().y;
                exec_floors(
                    ctx,
                    (1..y).rev(),
                    "Descended",
                    "No free spot below you found.",
                )
            }),
    );
}

fn register_selection(registry: &mut CommandRegistry) {
    registry.register(
        Command::new("/pos")
            .permission("worldedit.selection.pos")
            .require_plot_ownership()
            .syntax(opt([
                arg("pos1", ArgumentType::BlockPos),
                opt([arg("pos2", ArgumentType::BlockPos)]),
            ]))
            .executes(|ctx| {
                let pos1 = ctx
                    .block_position("pos1")?
                    .unwrap_or(ctx.player()?.pos.block_pos());
                let pos2 = ctx.block_position("pos2")?;

                let player = ctx.player_mut()?;
                player.worldedit_set_first_pos(pos1);
                if let Some(pos2) = pos2 {
                    player.worldedit_set_second_pos(pos2);
                }
                Ok(())
            }),
    );

    fn exec_pos(ctx: &mut ExecutionContext<'_>, first: bool) -> CommandResult<()> {
        let pos = ctx
            .block_position("coordinates")?
            .unwrap_or(ctx.player()?.pos.block_pos());
        set_pos(ctx, first, pos)
    }

    fn set_pos(ctx: &mut ExecutionContext<'_>, first: bool, pos: BlockPos) -> CommandResult<()> {
        let player = ctx.player_mut()?;
        if first {
            player.worldedit_set_first_pos(pos);
        } else {
            player.worldedit_set_second_pos(pos);
        }
        Ok(())
    }

    registry.register(
        Command::new("/pos1")
            .alias("/1")
            .permission("worldedit.selection.pos")
            .require_plot_ownership()
            .optional("coordinates", ArgumentType::BlockPos)
            .executes(|ctx| exec_pos(ctx, true)),
    );

    registry.register(
        Command::new("/pos2")
            .alias("/2")
            .permission("worldedit.selection.pos")
            .require_plot_ownership()
            .optional("coordinates", ArgumentType::BlockPos)
            .executes(|ctx| exec_pos(ctx, false)),
    );

    registry.register(
        Command::new("/hpos1")
            .alias("/h1")
            .permission("worldedit.selection.hpos")
            .require_plot_ownership()
            .executes(|ctx| {
                let pos = ctx.target_block(300.0)?;
                set_pos(ctx, true, pos)
            }),
    );

    registry.register(
        Command::new("/hpos2")
            .alias("/h2")
            .permission("worldedit.selection.hpos")
            .require_plot_ownership()
            .executes(|ctx| {
                let pos = ctx.target_block(300.0)?;
                set_pos(ctx, false, pos)
            }),
    );

    registry.register(
        Command::new("/sel")
            .alias(";")
            .alias("/desel")
            .alias("/deselect")
            .require_plot_ownership()
            .executes(|ctx| {
                ctx.player_mut()?.worldedit_clear_pos();
                Ok(())
            }),
    );

    fn exec_resize(ctx: &mut ExecutionContext<'_>, contract: bool) -> CommandResult<()> {
        let amount: i32 = ctx.arg_or("amount", 1)?;
        let reverse_amount = ctx.arg_opt::<i32>("reverseAmount")?;
        let direction = ctx.arg_or("direction", Direction::Me)?;

        let (first, second) = ctx.get_selection()?;
        let direction = direction.resolve(ctx.player()?.get_facing());

        let offset = direction.offset_pos(BlockPos::zero(), amount);
        let (mut new_first, mut new_second) =
            calculate_expanded_selection(first, second, offset, contract);

        if let Some(reverse_amount) = reverse_amount {
            let reverse_offset = direction.offset_pos(BlockPos::zero(), -reverse_amount);
            (new_first, new_second) =
                calculate_expanded_selection(new_first, new_second, reverse_offset, contract);
        }

        let old_volume = calculate_selection_volume(first, second);
        let new_volume = calculate_selection_volume(new_first, new_second);
        update_selection(ctx, (first, second), (new_first, new_second))?;

        if contract {
            ctx.worldedit_message(&format!(
                "Region contracted {} block(s).",
                old_volume - new_volume
            ))
        } else {
            ctx.worldedit_message(&format!(
                "Region expanded {} block(s).",
                new_volume - old_volume
            ))
        }
    }

    registry.register(
        Command::new("/expand")
            .alias("/e")
            .permission("worldedit.selection.expand")
            .require_plot_ownership()
            .subcommand(Command::new("vert").executes(|ctx| {
                let (first, second) = ctx.get_selection()?;
                let (mut new_first, mut new_second) = (first, second);
                let (lower, upper) = if first.y <= second.y {
                    (&mut new_first, &mut new_second)
                } else {
                    (&mut new_second, &mut new_first)
                };
                lower.y = 0;
                upper.y = PLOT_BLOCK_HEIGHT - 1;

                let total = calculate_selection_volume(new_first, new_second)
                    - calculate_selection_volume(first, second);
                update_selection(ctx, (first, second), (new_first, new_second))?;
                ctx.worldedit_message(&format!("Region expanded {} block(s).", total))
            }))
            .optional("amount", ArgumentType::integer(1, 1000))
            .optional("reverseAmount", ArgumentType::integer(1, 1000))
            .optional("direction", ArgumentType::Direction)
            .executes(|ctx| exec_resize(ctx, false)),
    );

    registry.register(
        Command::new("/contract")
            .permission("worldedit.selection.contract")
            .require_plot_ownership()
            .optional("amount", ArgumentType::integer(1, 1000))
            .optional("reverseAmount", ArgumentType::integer(1, 1000))
            .optional("direction", ArgumentType::Direction)
            .executes(|ctx| exec_resize(ctx, true)),
    );

    registry.register(
        Command::new("/shift")
            .permission("worldedit.selection.shift")
            .require_plot_ownership()
            .optional("amount", ArgumentType::integer(1, 1000))
            .optional("direction", ArgumentType::Direction)
            .executes(|ctx| {
                let amount: i32 = ctx.arg_or("amount", 1)?;
                let direction = ctx.arg_or("direction", Direction::Me)?;
                let direction = direction.resolve(ctx.player()?.get_facing());

                let (first, second) = ctx.get_selection()?;
                let offset = direction.offset_pos(BlockPos::zero(), amount);

                let player = ctx.player_mut()?;
                player.worldedit_set_first_pos(first + offset);
                player.worldedit_set_second_pos(second + offset);
                ctx.worldedit_message(&format!("Region shifted {} block(s).", amount))
            }),
    );
}

fn update_selection(
    ctx: &mut ExecutionContext<'_>,
    old: (BlockPos, BlockPos),
    new: (BlockPos, BlockPos),
) -> CommandResult<()> {
    let player = ctx.player_mut()?;
    if new.0 != old.0 {
        player.worldedit_set_first_pos(new.0);
    }
    if new.1 != old.1 {
        player.worldedit_set_second_pos(new.1);
    }
    Ok(())
}

fn exec_stack(
    ctx: &mut ExecutionContext<'_>,
    selection: (BlockPos, BlockPos),
    offset: BlockPos,
    ignore_air: bool,
) -> CommandResult<()> {
    let count = ctx.arg_or("count", 1)?;
    let shift_selection = ctx.flag("shift-selection");
    let expand_selection = ctx.flag("expand-selection");
    if ctx.flag("ignore-air") && ctx.flag("with-air") {
        return Err(CommandError::runtime(
            "Use either --ignore-air or --with-air, not both.",
        ));
    }
    if shift_selection && expand_selection {
        return Err(CommandError::runtime(
            "Use either --shift-selection or --expand-selection, not both.",
        ));
    }

    let start_time = Instant::now();
    let (first_pos, second_pos) = selection;
    let clipboard = create_clipboard(ctx.world(), first_pos, first_pos, second_pos);
    let destinations = (1..=count).rev().map(|i| {
        let offset = offset * i;
        (first_pos + offset, second_pos + offset)
    });
    let clipped = ctx.capture_undo_regions(destinations, first_pos)?;

    for i in 1..=count {
        paste_clipboard(
            ctx.world_mut(),
            &clipboard,
            first_pos + offset * i,
            ignore_air,
        );
    }

    let offset = offset * count;
    if shift_selection {
        update_selection(ctx, selection, (first_pos + offset, second_pos + offset))?;
    } else if expand_selection {
        let expanded = calculate_expanded_selection(first_pos, second_pos, offset, false);
        update_selection(ctx, selection, expanded)?;
    }

    let message = if clipped {
        "Your selection was stacked (clipped to plot bounds)."
    } else {
        "Your selection was stacked."
    };
    ctx.worldedit_message(&format!("{} ({:?})", message, start_time.elapsed()))
}

fn register_region(registry: &mut CommandRegistry) {
    fn exec_set(
        ctx: &mut ExecutionContext<'_>,
        mask: Option<&WorldEditMask>,
        pattern: &WorldEditPattern,
    ) -> CommandResult<()> {
        let (first_pos, second_pos) = ctx.get_selection()?;

        let origin = first_pos.min(second_pos);
        ctx.capture_undo_regions([(first_pos, second_pos)], origin)?;

        let start_time = Instant::now();
        let mut blocks_updated = 0;

        let world = ctx.world_mut();
        for pos in region_positions(first_pos, second_pos) {
            if mask.is_none_or(|mask| mask.matches(world, pos))
                && world.set_block_raw(pos, pattern.pick(world, pos).get_id())
            {
                blocks_updated += 1;
            }
        }

        ctx.worldedit_message(&format!(
            "Operation completed: {} block(s) affected ({:?})",
            blocks_updated,
            start_time.elapsed()
        ))
    }

    registry.register(
        Command::new("/set")
            .permission("worldedit.region.stack")
            .require_plot_ownership()
            .arg("pattern", ArgumentType::Pattern)
            .executes(|ctx| {
                let pattern = ctx.arg("pattern")?;
                exec_set(ctx, None, pattern)
            }),
    );

    registry.register(
        Command::new("/replace")
            .alias("/re")
            .alias("/rep")
            .permission("worldedit.region.replace")
            .require_plot_ownership()
            .arg("maskOrPattern", ArgumentType::Replacement)
            .optional("pattern", ArgumentType::Pattern)
            .executes(|ctx| {
                let first: &ReplacementOperand = ctx.arg("maskOrPattern")?;
                if let Some(pattern) = ctx.arg_opt("pattern")? {
                    let mask = first
                        .mask
                        .as_ref()
                        .ok_or_else(|| CommandError::runtime("The first operand must be a mask"))?;
                    return exec_set(ctx, Some(mask), pattern);
                }

                let pattern = first
                    .pattern
                    .as_ref()
                    .ok_or_else(|| CommandError::runtime("Expected a pattern after the mask"))?;
                exec_set(ctx, Some(&WorldEditMask::existing()), pattern)
            }),
    );

    registry.register(
        Command::new("/count")
            .permission("worldedit.analysis.count")
            .require_plot_ownership()
            .arg("mask", ArgumentType::Mask)
            .executes(|ctx| {
                let mask: &WorldEditMask = ctx.arg("mask")?;
                let (first_pos, second_pos) = ctx.get_selection()?;

                let start_time = Instant::now();
                let world = ctx.world();
                let blocks_counted = region_positions(first_pos, second_pos)
                    .filter(|&pos| mask.matches(world, pos))
                    .count();

                ctx.worldedit_message(&format!(
                    "Counted {} block(s) ({:?})",
                    blocks_counted,
                    start_time.elapsed()
                ))
            }),
    );

    registry.register(
        Command::new("/distr")
            .require_plot_ownership()
            .flag('c', "clipboard", "Get distribution of clipboard")
            .executes(|ctx| {
                let start_time = Instant::now();
                let mut block_counts = FxHashMap::default();

                if ctx.flag("clipboard") {
                    let clipboard = ctx.get_clipboard()?;
                    let volume = clipboard.size_x * clipboard.size_y * clipboard.size_z;
                    for i in 0..volume as usize {
                        *block_counts.entry(clipboard.data.get_entry(i)).or_insert(0) += 1;
                    }
                } else {
                    let (first_pos, second_pos) = ctx.get_selection()?;
                    let world = ctx.world();
                    for pos in region_positions(first_pos, second_pos) {
                        *block_counts
                            .entry(world.get_block(pos).get_id())
                            .or_insert(0) += 1;
                    }
                }

                let block_counts = block_counts
                    .into_iter()
                    .sorted_by_key(|(_, count)| *count)
                    .collect_vec();

                let total_blocks: u32 = block_counts.iter().map(|(_, count)| *count).sum();
                let mut result = format!("# total blocks: {}\n", total_blocks);
                for (block_id, count) in block_counts {
                    let percentage = (count as f64 / total_blocks as f64) * 100.0;
                    result.push_str(&format!(
                        "{:>6} ({:>5.2}%) {}\n",
                        count,
                        percentage,
                        Block::from_id(block_id).get_name()
                    ));
                }

                result.push_str(&format!("({:?})", start_time.elapsed()));
                ctx.worldedit_message(&result)
            }),
    );

    registry.register(
        Command::new("/stack")
            .alias("/s")
            .permission("worldedit.region.stack")
            .require_plot_ownership()
            .optional("count", ArgumentType::integer(1, 1000))
            .optional("direction", ArgumentType::Direction)
            .flag('a', "ignore-air", "Stack without air blocks")
            .flag(
                's',
                "shift-selection",
                "Shift the selection to the last stacked copy",
            )
            .executes(|ctx| {
                let direction = ctx.arg_or("direction", Direction::Me)?;
                let direction = direction.resolve(ctx.player()?.get_facing());
                let (first_pos, second_pos) = ctx.get_selection()?;
                let size = first_pos.max(second_pos) - first_pos.min(second_pos);
                let spacing = match direction {
                    BlockFacing::North | BlockFacing::South => size.z + 1,
                    BlockFacing::East | BlockFacing::West => size.x + 1,
                    BlockFacing::Up | BlockFacing::Down => size.y + 1,
                };
                let offset = direction.offset_pos(BlockPos::zero(), spacing);
                exec_stack(ctx, (first_pos, second_pos), offset, ctx.flag("ignore-air"))
            }),
    );
    registry.add_custom_alias("/sa", "/stack {} -a");

    registry.register(
        Command::new("/move")
            .permission("worldedit.region.move")
            .require_plot_ownership()
            .optional("count", ArgumentType::integer(1, 1000))
            .optional("direction", ArgumentType::Direction)
            .flag('a', "ignore-air", "Move without air blocks")
            .flag('s', "shift-selection", "Shift selection with the move")
            .executes(|ctx| {
                let count = ctx.arg_or("count", 1)?;
                let direction = ctx.arg_or("direction", Direction::Me)?;
                let direction = direction.resolve(ctx.player()?.get_facing());
                let ignore_air = ctx.flag("ignore-air");
                let shift_selection = ctx.flag("shift-selection");

                let (first_pos, second_pos) = ctx.get_selection()?;
                let start_time = Instant::now();

                let offset_amount = direction.offset_pos(BlockPos::zero(), count);
                let destination = (
                    first_pos.min(second_pos) + offset_amount,
                    first_pos.max(second_pos) + offset_amount,
                );
                if ctx.clip_region(destination.0, destination.1).is_none() {
                    return Err(RuntimeError::DestinationOutsidePlot.into());
                }

                let clipped = ctx.capture_undo_regions(
                    [(first_pos, second_pos), destination],
                    first_pos.min(second_pos),
                )?;

                let origin = BlockPos::zero();
                let clipboard = create_clipboard(ctx.world(), origin, first_pos, second_pos);
                clear_area(ctx.world_mut(), first_pos, second_pos);
                paste_clipboard(
                    ctx.world_mut(),
                    &clipboard,
                    origin + offset_amount,
                    ignore_air,
                );

                if shift_selection {
                    let player = ctx.player_mut()?;
                    player.worldedit_set_first_pos(first_pos + offset_amount);
                    player.worldedit_set_second_pos(second_pos + offset_amount);
                }

                let message = if clipped {
                    "Your selection was moved (clipped to plot bounds)."
                } else {
                    "Your selection was moved."
                };
                ctx.worldedit_message(&format!("{} ({:?})", message, start_time.elapsed()))
            }),
    );

    registry.register(
        Command::new("/rstack")
            .alias("/rs")
            .permission("redstonetools.rstack")
            .require_plot_ownership()
            .optional("count", ArgumentType::integer(1, 1000))
            .optional("direction", ArgumentType::DirectionWithDiagonals)
            .optional("offset", ArgumentType::integer(1, 1000))
            .flag('w', "with-air", "Stack with air blocks")
            .flag(
                's',
                "shift-selection",
                "Shift the selection to the last stacked copy",
            )
            .flag(
                'e',
                "expand-selection",
                "Expand selection to include stacked region",
            )
            .executes(|ctx| {
                let direction = ctx.arg_or("direction", DirectionWithDiagonals::Me)?;
                let spacing = ctx.arg_or("offset", 2)?;
                let player = ctx.player()?;
                let direction = direction.resolve(player.get_facing(), player.pitch);
                let selection = ctx.get_selection()?;
                exec_stack(ctx, selection, direction * spacing, !ctx.flag("with-air"))
            }),
    );

    registry.register(
        Command::new("/update")
            .permission("mchprs.we.update")
            .require_plot_ownership()
            .flag('p', "plot", "Update the entire plot")
            .executes(|ctx| {
                if ctx.flag("plot") {
                    let corners = ctx.world().get_corners();
                    update(ctx.world_mut(), corners.0, corners.1);
                    ctx.worldedit_message("Updated entire plot.")
                } else {
                    let (first_pos, second_pos) = ctx.get_selection()?;
                    update(ctx.world_mut(), first_pos, second_pos);
                    ctx.worldedit_message("Updated selection.")
                }
            }),
    );

    registry.register(
        Command::new("/replacecontainer")
            .alias("/rc")
            .permission("mchprs.we.replacecontainer")
            .require_plot_ownership()
            .arg("fromOrTo", ArgumentType::ContainerType)
            .optional("to", ArgumentType::ContainerType)
            .executes(exec_replacecontainer),
    );

    registry.register(
        Command::new("/wand")
            .permission("worldedit.wand")
            .require_plot_ownership()
            .executes(|ctx| {
                let item = ItemStack {
                    count: 1,
                    item_type: Item::WoodenAxe,
                    nbt: None,
                };
                let player = ctx.player_mut()?;
                let slot = 36 + player.selected_slot;
                player.set_inventory_slot(slot, Some(item.clone()));

                let entity_equipment = CSetEquipment {
                    entity_id: player.entity_id as i32,
                    equipment: vec![CSetEquipmentEquipment {
                        slot: 0,
                        item: Some(utils::encode_slot_data(&item)),
                    }],
                }
                .encode();

                for packet_sender in &ctx.world().packet_senders {
                    packet_sender.send_packet(&entity_equipment);
                }

                ctx.worldedit_message("Wand item given.")
            }),
    );
}

fn exec_replacecontainer(ctx: &mut ExecutionContext<'_>) -> CommandResult<()> {
    let first: ContainerType = ctx.arg("fromOrTo")?;
    let (from, to) = match ctx.arg_opt("to")? {
        Some(to) => (Some(first), to),
        None => (None, first),
    };
    let (first_pos, second_pos) = ctx.get_selection()?;

    let origin = first_pos.min(second_pos);
    ctx.capture_undo_regions([(first_pos, second_pos)], origin)?;

    let start_time = Instant::now();

    let new_block = match to {
        ContainerType::Furnace => Block::Furnace {
            facing: BlockDirection::North,
            lit: false,
        },
        ContainerType::Barrel => Block::Barrel {
            open: false,
            facing: BlockFacing::Up,
        },
        ContainerType::Hopper => Block::Hopper {
            enabled: false,
            facing: HopperFacing::Down,
        },
    };

    let world = ctx.world_mut();
    for pos in region_positions(first_pos, second_pos) {
        let block_ty = match world.get_block(pos) {
            Block::Furnace { .. } => ContainerType::Furnace,
            Block::Barrel { .. } => ContainerType::Barrel,
            Block::Hopper { .. } => ContainerType::Hopper,
            _ => continue,
        };

        let (current_ty, ss) = match world.get_block_entity(pos) {
            Some(BlockEntity::Container {
                comparator_override,
                ty,
                ..
            }) => (*ty, *comparator_override),
            _ => (block_ty, 0),
        };

        if from.is_some_and(|from| from != current_ty) {
            continue;
        }

        let items_needed = to.items_needed_for_signal_strength(ss);
        let inventory = (0..items_needed)
            .step_by(64)
            .enumerate()
            .map(|(slot, items_added)| InventoryEntry {
                id: Item::Redstone.get_id(),
                slot: slot as i8,
                count: (items_needed - items_added).min(64) as i8,
                nbt: None,
            })
            .collect();

        let new_entity = BlockEntity::Container {
            comparator_override: ss,
            inventory,
            ty: to,
        };
        world.set_block_entity(pos, new_entity);
        world.set_block(pos, new_block);
    }

    ctx.worldedit_message(&format!(
        "Your selection was replaced successfully. ({:?})",
        start_time.elapsed()
    ))
}

fn register_clipboard(registry: &mut CommandRegistry) {
    registry.register(
        Command::new("/copy")
            .alias("/c")
            .permission("worldedit.clipboard.copy")
            .require_plot_ownership()
            .executes(|ctx| {
                let (first_pos, second_pos) = ctx.get_selection()?;

                let start_time = Instant::now();
                let origin = ctx.player()?.pos.block_pos();
                let clipboard = create_clipboard(ctx.world(), origin, first_pos, second_pos);
                ctx.set_clipboard(clipboard)?;

                ctx.worldedit_message(&format!(
                    "Your selection was copied. ({:?})",
                    start_time.elapsed()
                ))
            }),
    );

    registry.register(
        Command::new("/cut")
            .alias("/x")
            .permission("worldedit.clipboard.cut")
            .require_plot_ownership()
            .executes(|ctx| {
                let (first_pos, second_pos) = ctx.get_selection()?;

                let origin = first_pos.min(second_pos);
                ctx.capture_undo_regions([(first_pos, second_pos)], origin)?;

                let start_time = Instant::now();
                let origin = ctx.player()?.pos.block_pos();
                let clipboard = create_clipboard(ctx.world(), origin, first_pos, second_pos);
                ctx.set_clipboard(clipboard)?;
                clear_area(ctx.world_mut(), first_pos, second_pos);

                ctx.worldedit_message(&format!(
                    "Your selection was cut. ({:?})",
                    start_time.elapsed()
                ))
            }),
    );

    registry.register(
        Command::new("/paste")
            .alias("/v")
            .permission("worldedit.clipboard.paste")
            .require_plot_ownership()
            .flag('a', "ignore-air", "Paste without air blocks")
            .flag('u', "update", "Update blocks after pasting")
            .flag('s', "select-region", "Select the pasted region")
            .flag('n', "no-paste", "No paste, select only")
            .executes(exec_paste),
    );
    registry.add_custom_alias("/va", "/paste -a");

    registry.register(
        Command::new("/flip")
            .alias("/f")
            .require_plot_ownership()
            .optional("direction", ArgumentType::Direction)
            .executes(|ctx| {
                let direction = ctx.arg_or("direction", Direction::Me)?;
                let direction = direction.resolve(ctx.player()?.get_facing());

                let start_time = Instant::now();
                let clipboard = ctx.get_clipboard()?;
                let (size_x, size_y, size_z) =
                    (clipboard.size_x, clipboard.size_y, clipboard.size_z);

                let flip_pos = |mut pos: BlockPos| {
                    match direction {
                        BlockFacing::East | BlockFacing::West => pos.x = size_x as i32 - 1 - pos.x,
                        BlockFacing::North | BlockFacing::South => {
                            pos.z = size_z as i32 - 1 - pos.z
                        }
                        BlockFacing::Up | BlockFacing::Down => pos.y = size_y as i32 - 1 - pos.y,
                    }
                    pos
                };
                let flip_block = |block: &mut Block| match direction {
                    BlockFacing::East | BlockFacing::West => block.flip(FlipDirection::FlipX),
                    BlockFacing::North | BlockFacing::South => block.flip(FlipDirection::FlipZ),
                    _ => {}
                };

                let flipped =
                    transform_clipboard(clipboard, (size_x, size_y, size_z), flip_pos, flip_block);
                ctx.set_clipboard(flipped)?;
                ctx.worldedit_message(&format!(
                    "The clipboard copy has been flipped. ({:?})",
                    start_time.elapsed()
                ))
            }),
    );

    registry.register(
        Command::new("/rotate")
            .alias("/r")
            .require_plot_ownership()
            .arg("angle", ArgumentType::integer(-360, 360))
            .executes(|ctx| {
                let angle: i32 = ctx.arg("angle")?;

                let start_time = Instant::now();
                let rotate_amt = match angle % 360 {
                    0 => {
                        return ctx.worldedit_message(
                            "Successfully rotated by 0! That took a lot of work.",
                        );
                    }
                    90 | -270 => RotateAmt::Rotate90,
                    180 | -180 => RotateAmt::Rotate180,
                    270 | -90 => RotateAmt::Rotate270,
                    _ => {
                        return Err(CommandError::runtime(
                            "Rotate amount must be a multiple of 90.",
                        ));
                    }
                };

                let clipboard = ctx.get_clipboard()?;
                let (size_x, size_y, size_z) =
                    (clipboard.size_x, clipboard.size_y, clipboard.size_z);
                let (n_size_x, n_size_z) = match rotate_amt {
                    RotateAmt::Rotate90 | RotateAmt::Rotate270 => (size_z, size_x),
                    _ => (size_x, size_z),
                };

                let rotate_pos = |pos: BlockPos| match rotate_amt {
                    RotateAmt::Rotate90 => BlockPos {
                        x: n_size_x as i32 - 1 - pos.z,
                        y: pos.y,
                        z: pos.x,
                    },
                    RotateAmt::Rotate180 => BlockPos {
                        x: n_size_x as i32 - 1 - pos.x,
                        y: pos.y,
                        z: n_size_z as i32 - 1 - pos.z,
                    },
                    RotateAmt::Rotate270 => BlockPos {
                        x: pos.z,
                        y: pos.y,
                        z: n_size_z as i32 - 1 - pos.x,
                    },
                };

                let rotated = transform_clipboard(
                    clipboard,
                    (n_size_x, size_y, n_size_z),
                    rotate_pos,
                    |block| block.rotate(rotate_amt),
                );
                ctx.set_clipboard(rotated)?;
                ctx.worldedit_message(&format!(
                    "The clipboard copy has been rotated. ({:?})",
                    start_time.elapsed()
                ))
            }),
    );
}

fn exec_paste(ctx: &mut ExecutionContext<'_>) -> CommandResult<()> {
    let clipboard = ctx.get_clipboard()?.clone();
    let ignore_air = ctx.flag("ignore-air");
    let should_update = ctx.flag("update");
    let select_region = ctx.flag("select-region");
    let no_paste = ctx.flag("no-paste");

    let start_time = Instant::now();

    let pos = ctx.player()?.pos.block_pos();
    let paste_min = pos - BlockPos::new(clipboard.offset_x, clipboard.offset_y, clipboard.offset_z);
    let paste_max = paste_min
        + BlockPos::new(
            clipboard.size_x as i32 - 1,
            clipboard.size_y as i32 - 1,
            clipboard.size_z as i32 - 1,
        );

    let clipped = if !no_paste {
        let clipped = ctx.capture_undo_regions([(paste_min, paste_max)], paste_min)?;
        paste_clipboard(ctx.world_mut(), &clipboard, pos, ignore_air);

        if should_update
            && let Some((region_min, region_max)) = ctx.clip_region(paste_min, paste_max)
        {
            update(ctx.world_mut(), region_min, region_max);
        }
        clipped
    } else {
        false
    };

    if select_region || no_paste {
        let player = ctx.player_mut()?;
        player.worldedit_set_first_pos(paste_min);
        player.worldedit_set_second_pos(paste_max);
    }

    let message = if no_paste {
        "Region selected."
    } else if clipped {
        "Your clipboard was pasted (clipped to plot bounds)."
    } else {
        "Your clipboard was pasted."
    };
    ctx.worldedit_message(&format!("{} ({:?})", message, start_time.elapsed()))
}

fn transform_clipboard(
    clipboard: &WorldEditClipboard,
    new_size: (u32, u32, u32),
    map_pos: impl Fn(BlockPos) -> BlockPos,
    map_block: impl Fn(&mut Block),
) -> WorldEditClipboard {
    let (size_x, size_y, size_z) = (clipboard.size_x, clipboard.size_y, clipboard.size_z);
    let (n_size_x, n_size_y, n_size_z) = new_size;
    let volume = size_x * size_y * size_z;
    let mut data = PalettedBitBuffer::new(volume as usize, 9);

    for y in 0..size_y {
        for z in 0..size_z {
            for x in 0..size_x {
                let i = y * size_x * size_z + z * size_x + x;
                let n = map_pos(BlockPos::new(x as i32, y as i32, z as i32));
                let n_i = n.y as u32 * n_size_x * n_size_z + n.z as u32 * n_size_x + n.x as u32;

                let mut block = Block::from_id(clipboard.data.get_entry(i as usize));
                map_block(&mut block);
                data.set_entry(n_i as usize, block.get_id());
            }
        }
    }

    let offset = map_pos(BlockPos::new(
        clipboard.offset_x,
        clipboard.offset_y,
        clipboard.offset_z,
    ));
    WorldEditClipboard {
        offset_x: offset.x,
        offset_y: offset.y,
        offset_z: offset.z,
        size_x: n_size_x,
        size_y: n_size_y,
        size_z: n_size_z,
        data,
        block_entities: clipboard
            .block_entities
            .iter()
            .map(|(pos, e)| (map_pos(*pos), e.clone()))
            .collect(),
    }
}

fn register_history(registry: &mut CommandRegistry) {
    registry.register(
        Command::new("/undo")
            .alias("undo")
            .permission("worldedit.history.undo")
            .require_plot_ownership()
            .optional("times", ArgumentType::integer(1, 100))
            .executes(|ctx| exec_history(ctx, true)),
    );

    registry.register(
        Command::new("/redo")
            .alias("redo")
            .permission("worldedit.history.redo")
            .require_plot_ownership()
            .optional("times", ArgumentType::integer(1, 100))
            .executes(|ctx| exec_history(ctx, false)),
    );
}

fn exec_history(ctx: &mut ExecutionContext<'_>, undo: bool) -> CommandResult<()> {
    let times: i32 = ctx.arg_or("times", 1)?;
    let (plot_x, plot_z) = (ctx.world().x, ctx.world().z);

    let mut done = 0;
    let result = loop {
        if done == times {
            break Ok(());
        }
        let player = ctx.player_mut()?;
        let stack = if undo {
            &mut player.worldedit_undo
        } else {
            &mut player.worldedit_redo
        };
        let Some(entry) = stack.last() else {
            break Ok(());
        };
        if entry.plot_x != plot_x || entry.plot_z != plot_z {
            break Err(if undo {
                RuntimeError::UndoFromDifferentPlot
            } else {
                RuntimeError::RedoFromDifferentPlot
            });
        }
        let entry = stack.pop().unwrap();

        let inverse = apply_history(ctx.world_mut(), &entry);
        let player = ctx.player_mut()?;
        if undo {
            player.worldedit_redo.push(inverse);
        } else {
            player.worldedit_undo.push(inverse);
        }
        done += 1;
    };

    let (verb, past, none_left) = if undo {
        ("Undo", "Undid", RuntimeError::NoUndoHistory)
    } else {
        ("Redo", "Redid", RuntimeError::NoRedoHistory)
    };
    match done {
        0 => return result.and(Err(none_left)).map_err(Into::into),
        1 => ctx.worldedit_message(&format!("{} successful.", verb))?,
        n => ctx.worldedit_message(&format!("{} {} operations.", past, n))?,
    }
    result.map_err(Into::into)
}

fn apply_history(world: &mut PlotWorld, entry: &WorldEditUndo) -> WorldEditUndo {
    let inverse_clipboards = entry
        .clipboards
        .iter()
        .map(|cb| {
            let offset = BlockPos::new(cb.offset_x, cb.offset_y, cb.offset_z);
            let first_pos = entry.pos - offset;
            let size = BlockPos::new(
                cb.size_x as i32 - 1,
                cb.size_y as i32 - 1,
                cb.size_z as i32 - 1,
            );
            create_clipboard(world, entry.pos, first_pos, first_pos + size)
        })
        .collect();

    for clipboard in &entry.clipboards {
        paste_clipboard(world, clipboard, entry.pos, false);
    }

    WorldEditUndo {
        clipboards: inverse_clipboards,
        pos: entry.pos,
        plot_x: entry.plot_x,
        plot_z: entry.plot_z,
    }
}

fn register_schematic(registry: &mut CommandRegistry) {
    registry.register(
        Command::new("schematic")
            .alias("schem")
            .alias("/schematic")
            .alias("/schem")
            .subcommand(
                Command::new("list")
                    .alias("all")
                    .alias("ls")
                    .executes(|ctx| {
                        let schematics = schematic_names(ctx.player()?.uuid).map_err(|error| {
                            warn!("Unable to list schematics: {error}");
                            CommandError::runtime("Unable to list schematics. Check console for details.")
                        })?;

                        if schematics.is_empty() {
                            return ctx.worldedit_message("No schematics found.");
                        }
                        ctx.worldedit_message(&format!(
                            "Available schematics ({}): {}",
                            schematics.len(),
                            schematics.join(", ")
                        ))
                    }),
            )
            .subcommand(
                Command::new("load")
                    .permission("worldedit.clipboard.load")
                    .require_plot_ownership()
                    .arg("filename", ArgumentType::SchematicFile)
                    .executes(|ctx| {
                        let filename: String = ctx.arg("filename")?;
                        let start_time = Instant::now();
                        let path = schematic_path(ctx.player()?.uuid, &filename)?;

                        match load_schematic(&path) {
                            Ok(clipboard) => {
                                ctx.set_clipboard(clipboard)?;
                                ctx.worldedit_message(&format!(
                                    "The schematic was loaded to your clipboard. Do //paste to birth it into the world. ({:?})",
                                    start_time.elapsed()
                                ))
                            }
                            Err(e) => {
                                if let Some(io_err) = e.downcast_ref::<io::Error>()
                                    && io_err.kind() == io::ErrorKind::NotFound
                                {
                                    return Err(CommandError::runtime(
                                        "The specified schematic file could not be found.",
                                    ));
                                }
                                warn!("There was an error loading a schematic:");
                                warn!("{}", e);
                                Err(CommandError::runtime(
                                    "There was an error loading the schematic. Check console for more details.",
                                ))
                            }
                        }
                    }),
            )
            .subcommand(
                Command::new("save")
                    .permission("worldedit.clipboard.save")
                    .require_plot_ownership()
                    .arg("filename", ArgumentType::Word)
                    .flag('f', "force-overwrite", "Overwrite existing file")
                    .executes(|ctx| {
                        let filename: String = ctx.arg("filename")?;
                        let start_time = Instant::now();

                        let path = schematic_path(ctx.player()?.uuid, &filename)?;
                        let clipboard = ctx.get_clipboard()?;
                        match save_schematic(&path, clipboard, ctx.flag("force-overwrite")) {
                            Ok(_) => ctx.worldedit_message(&format!(
                                "The schematic was saved successfully. ({:?})",
                                start_time.elapsed()
                            )),
                            Err(e) => {
                                if e.downcast_ref::<io::Error>().is_some_and(|error| error.kind() == io::ErrorKind::AlreadyExists) {
                                    return Err(CommandError::runtime("File already exists. Use -f flag to overwrite."));
                                }
                                warn!("There was an error saving a schematic:");
                                warn!("{:?}", e);
                                Err(CommandError::runtime(
                                    "There was an error saving the schematic.",
                                ))
                            }
                        }
                    }),
            ),
    );
    registry.add_custom_alias("/load", "/schematic load");
    registry.add_custom_alias("/save", "/schematic save");
}
