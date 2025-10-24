use crate::{
    commands::{
        argument::ArgumentType,
        context::ExecutionContext,
        error::{CommandError, CommandResult, RuntimeError},
        node::CommandNode,
        registry::CommandRegistry,
        value::{Direction, DirectionExt},
    },
    config::CONFIG,
    plot::PLOT_BLOCK_HEIGHT,
    utils::{self, HyphenatedUUID},
    worldedit::{
        calculate_expanded_selection, calculate_selection_volume, clear_area, create_clipboard,
        paste_clipboard, ray_trace_block,
        schematic::{load_schematic, save_schematic},
        update, WorldEditClipboard, WorldEditPattern, WorldEditUndo,
    },
};
use mchprs_blocks::{
    block_entities::{BlockEntity, ContainerType, InventoryEntry},
    blocks::{Block, FlipDirection, RotateAmt},
    items::{Item, ItemStack},
    BlockFacing, BlockPos,
};
use mchprs_network::packets::clientbound::*;
use mchprs_world::{storage::PalettedBitBuffer, World};
use once_cell::sync::Lazy;
use regex::Regex;
use std::time::Instant;
use tracing::warn;

static SCHEMATI_VALIDATE_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[a-zA-Z0-9_.]+\.schem(atic)?").unwrap());

pub(super) fn register_commands(registry: &mut CommandRegistry) {
    fn exec_up(ctx: &mut ExecutionContext<'_>, distance: i32) -> CommandResult<()> {
        let flags = ctx.args().get_flags("flags").unwrap_or_default();
        let force_flight = flags.contains("force-flight");
        let force_glass = flags.contains("force-glass");

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
            if matches!(ctx.world().get_block(platform_pos), Block::Air {}) {
                ctx.world_mut().set_block(platform_pos, Block::Glass {});
            }
        }

        ctx.player_mut()?.teleport(new_pos);
        ctx.worldedit_message(&format!("Moved up {} blocks", distance))?;
        Ok(())
    }

    let up_flags_arg = ArgumentType::flags()
        .add('f', "force-flight", "Force using flight to keep you still")
        .add('g', "force-glass", "Force using glass to keep you still");

    registry.register(
        CommandNode::literal("up")
            .alias("u")
            .require_permission("worldedit.navigation.up")
            .mutates_world()
            .then(
                CommandNode::argument("distance", ArgumentType::integer(1, 100)).then(
                    CommandNode::argument("flags", up_flags_arg.clone()).executes(|ctx| {
                        let distance = ctx.args().get_integer("distance")?;
                        exec_up(ctx, distance)
                    }),
                ),
            ),
    );

    fn exec_ascend(ctx: &mut ExecutionContext<'_>, initial_levels: i32) -> CommandResult<()> {
        let mut levels = initial_levels;

        let player_pos = ctx.player()?.pos.block_pos();
        let mut player_y = player_pos.y;

        for y in player_y + 1..=PLOT_BLOCK_HEIGHT {
            if levels == 0 {
                break;
            }

            let floor_pos = BlockPos::new(player_pos.x, y - 1, player_pos.z);
            let pos = BlockPos::new(player_pos.x, y, player_pos.z);
            let high_pos = BlockPos::new(player_pos.x, y + 1, player_pos.z);

            if ctx.world().get_block(floor_pos) != (Block::Air {})
                && ctx.world().get_block(pos) == (Block::Air {})
                && ctx.world().get_block(high_pos) == (Block::Air {})
            {
                player_y = y;
                levels -= 1;
            }
        }

        if player_y == player_pos.y {
            return Err(CommandError::runtime("No free spot above you found."));
        }

        let mut new_pos = ctx.player()?.pos;
        new_pos.y = player_y as f64;
        ctx.player_mut()?.teleport(new_pos);
        ctx.worldedit_message(&format!("Ascended {} levels", initial_levels - levels))?;
        Ok(())
    }

    registry.register(
        CommandNode::literal("ascend")
            .alias("asc")
            .require_permission("worldedit.navigation.ascend")
            .executes(|ctx| exec_ascend(ctx, 1))
            .then(
                CommandNode::argument("levels", ArgumentType::integer(1, 100)).executes(|ctx| {
                    let levels = ctx.args().get_integer("levels")?;
                    exec_ascend(ctx, levels)
                }),
            ),
    );

    fn exec_descend(ctx: &mut ExecutionContext<'_>, initial_levels: i32) -> CommandResult<()> {
        let mut levels = initial_levels;

        let player_pos = ctx.player()?.pos.block_pos();
        let mut player_y = player_pos.y;

        for y in (1..player_pos.y).rev() {
            if levels == 0 {
                break;
            }

            let floor_pos = BlockPos::new(player_pos.x, y - 1, player_pos.z);
            let pos = BlockPos::new(player_pos.x, y, player_pos.z);
            let high_pos = BlockPos::new(player_pos.x, y + 1, player_pos.z);

            if ctx.world().get_block(floor_pos) != (Block::Air {})
                && ctx.world().get_block(pos) == (Block::Air {})
                && ctx.world().get_block(high_pos) == (Block::Air {})
            {
                player_y = y;
                levels -= 1;
            }
        }

        if player_y == player_pos.y {
            return Err(CommandError::runtime("No free spot below you found."));
        }

        let mut new_pos = ctx.player()?.pos;
        new_pos.y = player_y as f64;
        ctx.player_mut()?.teleport(new_pos);
        ctx.worldedit_message(&format!("Descended {} levels", initial_levels - levels))?;
        Ok(())
    }

    registry.register(
        CommandNode::literal("descend")
            .alias("desc")
            .require_permission("worldedit.navigation.descend")
            .executes(|ctx| exec_descend(ctx, 1))
            .then(
                CommandNode::argument("levels", ArgumentType::integer(1, 100)).executes(|ctx| {
                    let levels = ctx.args().get_integer("levels")?;
                    exec_descend(ctx, levels)
                }),
            ),
    );

    registry.register(
        CommandNode::literal("/pos1")
            .alias("/1")
            .require_permission("worldedit.selection.pos")
            .require_plot_ownership()
            .executes(|ctx| {
                let pos = ctx.player()?.pos.block_pos();
                ctx.player_mut()?.worldedit_set_first_pos(pos);
                Ok(())
            })
            .then(
                CommandNode::argument("coordinates", ArgumentType::block_pos()).executes(|ctx| {
                    let relative_pos = ctx.args().get_block_pos("coordinates")?;
                    let player = ctx.player_mut()?;
                    let pos = relative_pos.resolve(player.pos.block_pos());
                    player.worldedit_set_first_pos(pos);
                    Ok(())
                }),
            ),
    );

    registry.register(
        CommandNode::literal("/pos2")
            .alias("/2")
            .require_permission("worldedit.selection.pos")
            .require_plot_ownership()
            .executes(|ctx| {
                let pos = ctx.player()?.pos.block_pos();
                ctx.player_mut()?.worldedit_set_second_pos(pos);
                Ok(())
            })
            .then(
                CommandNode::argument("coordinates", ArgumentType::block_pos()).executes(|ctx| {
                    let relative_pos = ctx.args().get_block_pos("coordinates")?;
                    let player = ctx.player_mut()?;
                    let pos = relative_pos.resolve(player.pos.block_pos());
                    player.worldedit_set_second_pos(pos);
                    Ok(())
                }),
            ),
    );

    registry.register(
        CommandNode::literal("/hpos1")
            .alias("/h1")
            .require_permission("worldedit.selection.hpos")
            .require_plot_ownership()
            .executes(|ctx| {
                let player = ctx.player()?;
                let pitch = player.pitch as f64;
                let yaw = player.yaw as f64;
                let player_pos = player.pos;
                let pos = match ray_trace_block(ctx.world(), player_pos, pitch, yaw, 300.0) {
                    Some(pos) => pos,
                    None => return Err(RuntimeError::NoBlockInSight.into()),
                };
                ctx.player_mut()?.worldedit_set_first_pos(pos);
                Ok(())
            }),
    );

    registry.register(
        CommandNode::literal("/hpos2")
            .alias("/h2")
            .require_permission("worldedit.selection.hpos")
            .require_plot_ownership()
            .executes(|ctx| {
                let player = ctx.player()?;
                let pitch = player.pitch as f64;
                let yaw = player.yaw as f64;
                let player_pos = player.pos;
                let pos = match ray_trace_block(ctx.world(), player_pos, pitch, yaw, 300.0) {
                    Some(pos) => pos,
                    None => return Err(RuntimeError::NoBlockInSight.into()),
                };
                ctx.player_mut()?.worldedit_set_second_pos(pos);
                Ok(())
            }),
    );

    registry.register(
        CommandNode::literal("/sel")
            .alias(";")
            .alias("/desel")
            .alias("/deselect")
            .require_permission("worldedit.selection.pos")
            .require_plot_ownership()
            .executes(|ctx| {
                ctx.player_mut()?.worldedit_clear_pos();
                Ok(())
            }),
    );

    registry.register(
        CommandNode::literal("/set")
            .require_permission("worldedit.region.set")
            .require_plot_ownership()
            .mutates_world()
            .then(
                CommandNode::argument("pattern", ArgumentType::pattern()).executes(|ctx| {
                    let pattern = ctx.args().get_pattern("pattern")?;
                    let (first_pos, second_pos) = ctx.get_selection()?;

                    let origin = first_pos.min(second_pos);
                    ctx.capture_undo_regions([(first_pos, second_pos)], origin, true)?;

                    let start_time = Instant::now();
                    let mut blocks_updated = 0;

                    let start_pos = first_pos.min(second_pos);
                    let end_pos = first_pos.max(second_pos);

                    for x in start_pos.x..=end_pos.x {
                        for y in start_pos.y..=end_pos.y {
                            for z in start_pos.z..=end_pos.z {
                                let block_pos = BlockPos::new(x, y, z);
                                let block_id = pattern.pick().get_id();

                                if ctx.world_mut().set_block_raw(block_pos, block_id) {
                                    blocks_updated += 1;
                                }
                            }
                        }
                    }

                    ctx.worldedit_message(&format!(
                        "Operation completed: {} block(s) affected ({:?})",
                        blocks_updated,
                        start_time.elapsed()
                    ))
                }),
            ),
    );

    fn exec_replace(
        ctx: &mut ExecutionContext<'_>,
        mask: Option<WorldEditPattern>,
        pattern: WorldEditPattern,
    ) -> CommandResult<()> {
        let (first_pos, second_pos) = ctx.get_selection()?;

        let origin = first_pos.min(second_pos);
        ctx.capture_undo_regions([(first_pos, second_pos)], origin, true)?;

        let start_time = Instant::now();
        let mut blocks_updated = 0;

        let start_pos = first_pos.min(second_pos);
        let end_pos = first_pos.max(second_pos);

        for x in start_pos.x..=end_pos.x {
            for y in start_pos.y..=end_pos.y {
                for z in start_pos.z..=end_pos.z {
                    let block_pos = BlockPos::new(x, y, z);

                    let should_replace = mask
                        .as_ref()
                        .is_none_or(|mask| mask.matches(ctx.world().get_block(block_pos)));

                    if should_replace {
                        let block_id = pattern.pick().get_id();

                        if ctx.world_mut().set_block_raw(block_pos, block_id) {
                            blocks_updated += 1;
                        }
                    }
                }
            }
        }

        ctx.worldedit_message(&format!(
            "Operation completed: {} block(s) affected ({:?})",
            blocks_updated,
            start_time.elapsed()
        ))
    }

    registry.register(
        CommandNode::literal("/replace")
            .alias("/re")
            .alias("/rep")
            .require_permission("worldedit.region.replace")
            .require_plot_ownership()
            .mutates_world()
            .then(
                CommandNode::argument("to", ArgumentType::pattern()).executes(|ctx| {
                    let pattern = ctx.args().get_pattern("to")?;
                    exec_replace(ctx, None, pattern)
                }),
            )
            .then(CommandNode::argument("from", ArgumentType::mask()).then(
                CommandNode::argument("to", ArgumentType::pattern()).executes(|ctx| {
                    let mask = ctx.args().get_mask("from")?;
                    let pattern = ctx.args().get_pattern("to")?;
                    exec_replace(ctx, Some(mask), pattern)
                }),
            )),
    );

    registry.register(
        CommandNode::literal("/count")
            .require_permission("worldedit.analysis.count")
            .require_plot_ownership()
            .then(
                CommandNode::argument("mask", ArgumentType::mask()).executes(|ctx| {
                    let mask = ctx.args().get_mask("mask")?;
                    let (first_pos, second_pos) = ctx.get_selection()?;

                    let start_time = Instant::now();
                    let mut blocks_counted = 0;

                    let start_pos = first_pos.min(second_pos);
                    let end_pos = first_pos.max(second_pos);

                    for x in start_pos.x..=end_pos.x {
                        for y in start_pos.y..=end_pos.y {
                            for z in start_pos.z..=end_pos.z {
                                let block_pos = BlockPos::new(x, y, z);
                                if mask.matches(ctx.world().get_block(block_pos)) {
                                    blocks_counted += 1;
                                }
                            }
                        }
                    }

                    ctx.worldedit_message(&format!(
                        "Counted {} block(s) ({:?})",
                        blocks_counted,
                        start_time.elapsed()
                    ))
                }),
            ),
    );

    fn exec_copy(ctx: &mut ExecutionContext<'_>) -> CommandResult<()> {
        let (first_pos, second_pos) = ctx.get_selection()?;

        let start_time = Instant::now();
        let origin = ctx.player()?.pos.block_pos();
        let clipboard = create_clipboard(ctx.world_mut(), origin, first_pos, second_pos);

        ctx.set_clipboard(clipboard)?;

        ctx.worldedit_message(&format!(
            "Your selection was copied. ({:?})",
            start_time.elapsed()
        ))
    }

    registry.register(
        CommandNode::literal("/copy")
            .alias("/c")
            .require_permission("worldedit.clipboard.copy")
            .require_plot_ownership()
            .executes(exec_copy),
    );

    fn exec_cut(ctx: &mut ExecutionContext<'_>) -> CommandResult<()> {
        let (first_pos, second_pos) = ctx.get_selection()?;

        let origin = first_pos.min(second_pos);
        ctx.capture_undo_regions([(first_pos, second_pos)], origin, true)?;

        let start_time = Instant::now();
        let origin = ctx.player()?.pos.block_pos();
        let clipboard = create_clipboard(ctx.world_mut(), origin, first_pos, second_pos);

        ctx.set_clipboard(clipboard)?;
        clear_area(ctx.world_mut(), first_pos, second_pos);

        ctx.worldedit_message(&format!(
            "Your selection was cut. ({:?})",
            start_time.elapsed()
        ))
    }

    registry.register(
        CommandNode::literal("/cut")
            .alias("/x")
            .require_permission("worldedit.clipboard.cut")
            .require_plot_ownership()
            .mutates_world()
            .executes(|ctx| exec_cut(ctx)),
    );

    fn exec_paste(ctx: &mut ExecutionContext<'_>) -> CommandResult<()> {
        let clipboard = ctx.get_clipboard()?.clone();

        let flags = ctx.args().get_flags("flags")?;
        let ignore_air = flags.contains("ignore-air");
        let should_update = flags.contains("update");
        let original_position = flags.contains("original-position");
        let select_region = flags.contains("select-region");
        let no_paste = flags.contains("no-paste");

        let start_time = Instant::now();

        let pos = if original_position {
            BlockPos::new(clipboard.offset_x, clipboard.offset_y, clipboard.offset_z)
        } else {
            ctx.player()?.pos.block_pos()
        };

        let paste_min =
            pos - BlockPos::new(clipboard.offset_x, clipboard.offset_y, clipboard.offset_z);

        let paste_max = paste_min
            + BlockPos::new(
                clipboard.size_x as i32 - 1,
                clipboard.size_y as i32 - 1,
                clipboard.size_z as i32 - 1,
            );

        if !no_paste {
            ctx.capture_undo_regions([(paste_min, paste_max)], paste_min, true)?;
            paste_clipboard(ctx.world_mut(), &clipboard, pos, ignore_air);

            if should_update {
                update(ctx.world_mut(), paste_min, paste_max);
            }
        }

        if select_region || no_paste {
            let player = ctx.player_mut()?;
            player.worldedit_set_first_pos(paste_min);
            player.worldedit_set_second_pos(paste_max);
        }

        let message = if no_paste {
            "Region selected."
        } else {
            "Your clipboard was pasted."
        };

        ctx.worldedit_message(&format!("{} ({:?})", message, start_time.elapsed()))
    }

    let paste_flags_arg = ArgumentType::flags()
        .add('a', "ignore-air", "Paste without air blocks")
        .add('u', "update", "Update blocks after pasting")
        .add('o', "original-position", "Paste at original position")
        .add('s', "select-region", "Select the pasted region")
        .add('n', "no-paste", "No paste, select only");

    registry.register(
        CommandNode::literal("/paste")
            .alias("/v")
            .require_permission("worldedit.clipboard.paste")
            .require_plot_ownership()
            .mutates_world()
            .then(CommandNode::argument("flags", paste_flags_arg.clone()).executes(exec_paste)),
    );
    registry.add_custom_alias("/va", "/paste -a");

    fn exec_undo_single(ctx: &mut ExecutionContext<'_>) -> CommandResult<bool> {
        let player = ctx.player_mut()?;
        let Some(undo) = player.worldedit_undo.pop() else {
            return Ok(false);
        };

        let world = ctx.world_mut();
        let plot_x = world.x;
        let plot_z = world.z;

        if undo.plot_x != plot_x || undo.plot_z != plot_z {
            return Err(RuntimeError::UndoFromDifferentPlot.into());
        }

        let redo_clipboards: Vec<WorldEditClipboard> = undo
            .clipboards
            .iter()
            .map(|cb| {
                let offset = BlockPos::new(cb.offset_x, cb.offset_y, cb.offset_z);
                let first_pos = undo.pos - offset;
                let size = BlockPos::new(
                    cb.size_x as i32 - 1,
                    cb.size_y as i32 - 1,
                    cb.size_z as i32 - 1,
                );
                let second_pos = first_pos + size;
                create_clipboard(world, undo.pos, first_pos, second_pos)
            })
            .collect();

        for clipboard in &undo.clipboards {
            paste_clipboard(world, clipboard, undo.pos, false);
        }

        let redo = WorldEditUndo {
            clipboards: redo_clipboards,
            pos: undo.pos,
            plot_x,
            plot_z,
        };
        ctx.player_mut()?.worldedit_redo.push(redo);

        Ok(true)
    }

    fn exec_undo(ctx: &mut ExecutionContext<'_>, times: i32) -> CommandResult<()> {
        let mut undone = 0;

        for _ in 0..times {
            if exec_undo_single(ctx)? {
                undone += 1;
            } else {
                break;
            }
        }

        match undone {
            0 => Err(RuntimeError::NoUndoHistory.into()),
            1 => ctx.worldedit_message("Undo successful."),
            n => ctx.worldedit_message(&format!("Undid {} operations.", n)),
        }
    }

    registry.register(
        CommandNode::literal("/undo")
            .alias("undo")
            .require_permission("worldedit.history.undo")
            .require_plot_ownership()
            .mutates_world()
            .executes(|ctx| exec_undo(ctx, 1))
            .then(
                CommandNode::argument("times", ArgumentType::integer(1, 100)).executes(|ctx| {
                    let times = ctx.args().get_integer("times")?;
                    exec_undo(ctx, times)
                }),
            ),
    );

    fn exec_redo_single(ctx: &mut ExecutionContext<'_>) -> CommandResult<bool> {
        let player = ctx.player_mut()?;
        let Some(redo) = player.worldedit_redo.pop() else {
            return Ok(false);
        };

        let plot_x = ctx.world().x;
        let plot_z = ctx.world().z;

        if redo.plot_x != plot_x || redo.plot_z != plot_z {
            return Err(RuntimeError::RedoFromDifferentPlot.into());
        }

        let undo_clipboards: Vec<WorldEditClipboard> = redo
            .clipboards
            .iter()
            .map(|cb| {
                let offset = BlockPos::new(cb.offset_x, cb.offset_y, cb.offset_z);
                let first_pos = redo.pos - offset;
                let size = BlockPos::new(
                    cb.size_x as i32 - 1,
                    cb.size_y as i32 - 1,
                    cb.size_z as i32 - 1,
                );
                let second_pos = first_pos + size;
                create_clipboard(ctx.world_mut(), redo.pos, first_pos, second_pos)
            })
            .collect();

        for clipboard in &redo.clipboards {
            paste_clipboard(ctx.world_mut(), clipboard, redo.pos, false);
        }

        let undo = WorldEditUndo {
            clipboards: undo_clipboards,
            pos: redo.pos,
            plot_x,
            plot_z,
        };
        ctx.player_mut()?.worldedit_undo.push(undo);

        Ok(true)
    }

    fn exec_redo(ctx: &mut ExecutionContext<'_>, times: i32) -> CommandResult<()> {
        let mut redone = 0;

        for _ in 0..times {
            if exec_redo_single(ctx)? {
                redone += 1;
            } else {
                break;
            }
        }

        match redone {
            0 => Err(RuntimeError::NoRedoHistory.into()),
            1 => ctx.worldedit_message("Redo successful."),
            n => ctx.worldedit_message(&format!("Redid {} operations.", n)),
        }
    }

    registry.register(
        CommandNode::literal("/redo")
            .alias("redo")
            .require_permission("worldedit.history.redo")
            .require_plot_ownership()
            .mutates_world()
            .executes(|ctx| exec_redo(ctx, 1))
            .then(
                CommandNode::argument("times", ArgumentType::integer(1, 100)).executes(|ctx| {
                    let times = ctx.args().get_integer("times")?;
                    exec_redo(ctx, times)
                }),
            ),
    );

    fn exec_stack(
        ctx: &mut ExecutionContext<'_>,
        count: i32,
        offset: Option<i32>,
        direction: Direction,
    ) -> CommandResult<()> {
        let player = ctx.player()?;
        let direction = direction.resolve(player.get_facing());

        let flags = ctx.args().get_flags("flags")?;
        let ignore_air = flags.contains("ignore-air");
        let shift_selection = flags.contains("shift-selection");

        let (first_pos, second_pos) = ctx.get_selection()?;
        let start_time = Instant::now();

        let clipboard = create_clipboard(ctx.world_mut(), first_pos, first_pos, second_pos);

        let stack_offset = offset.unwrap_or_else(|| match direction {
            BlockFacing::North | BlockFacing::South => clipboard.size_z as i32,
            BlockFacing::East | BlockFacing::West => clipboard.size_x as i32,
            BlockFacing::Up | BlockFacing::Down => clipboard.size_y as i32,
        });

        let undo_positions = (1..=count).map(|i| {
            let offset = i * stack_offset;
            (
                direction.offset_pos(first_pos, offset),
                direction.offset_pos(second_pos, offset),
            )
        });
        ctx.capture_undo_regions(undo_positions, first_pos, true)?;

        for i in 1..=count {
            let offset = i * stack_offset;
            let paste_pos = direction.offset_pos(first_pos, offset);
            paste_clipboard(ctx.world_mut(), &clipboard, paste_pos, ignore_air);
        }

        if shift_selection {
            let offset_amount = direction.offset_pos(BlockPos::zero(), count * stack_offset);
            let player = ctx.player_mut()?;
            player.worldedit_set_first_pos(first_pos + offset_amount);
            player.worldedit_set_second_pos(second_pos + offset_amount);
        }

        ctx.worldedit_message(&format!(
            "Your selection was stacked. ({:?})",
            start_time.elapsed()
        ))
    }

    let stack_flags_arg = ArgumentType::flags()
        .add('a', "ignore-air", "Stack without air blocks")
        .add(
            's',
            "shift-selection",
            "Shift the selection to the last stacked copy",
        );

    registry.register(
        CommandNode::literal("/stack")
            .alias("/s")
            .require_permission("worldedit.region.stack")
            .require_plot_ownership()
            .mutates_world()
            .then(
                CommandNode::argument("count", ArgumentType::integer(1, 1000))
                    .then(
                        CommandNode::argument("offset", ArgumentType::integer(1, 1000))
                            .then(
                                CommandNode::argument("direction", ArgumentType::direction()).then(
                                    CommandNode::argument("flags", stack_flags_arg.clone())
                                        .executes(|ctx| {
                                            let count = ctx.args().get_integer("count")?;
                                            let offset = ctx.args().get_integer("offset")?;
                                            let direction =
                                                ctx.args().get_direction("direction")?;
                                            exec_stack(ctx, count, Some(offset), direction)
                                        }),
                                ),
                            )
                            .then(
                                CommandNode::argument("flags", stack_flags_arg.clone()).executes(
                                    |ctx| {
                                        let count = ctx.args().get_integer("count")?;
                                        let offset = ctx.args().get_integer("offset")?;
                                        exec_stack(ctx, count, Some(offset), Direction::Me)
                                    },
                                ),
                            ),
                    )
                    .then(
                        CommandNode::argument("direction", ArgumentType::direction()).then(
                            CommandNode::argument("flags", stack_flags_arg.clone()).executes(
                                |ctx| {
                                    let count = ctx.args().get_integer("count")?;
                                    let direction = ctx.args().get_direction("direction")?;
                                    exec_stack(ctx, count, None, direction)
                                },
                            ),
                        ),
                    )
                    .then(
                        CommandNode::argument("flags", stack_flags_arg.clone()).executes(|ctx| {
                            let count = ctx.args().get_integer("count")?;
                            exec_stack(ctx, count, None, Direction::Me)
                        }),
                    ),
            )
            .then(
                CommandNode::argument("flags", stack_flags_arg.clone())
                    .executes(|ctx| exec_stack(ctx, 1, None, Direction::Me)),
            ),
    );
    registry.add_custom_alias("/sa", "/stack {} -a");

    fn exec_move(
        ctx: &mut ExecutionContext<'_>,
        count: i32,
        direction: Direction,
    ) -> CommandResult<()> {
        let player = ctx.player()?;
        let direction = direction.resolve(player.get_facing());

        let flags = ctx.args().get_flags("flags")?;
        let ignore_air = flags.contains("ignore-air");
        let shift_selection = flags.contains("shift-selection");

        let (first_pos, second_pos) = ctx.get_selection()?;
        let start_time = Instant::now();

        let offset_amount = direction.offset_pos(BlockPos::zero(), count);

        ctx.capture_undo_regions(
            [
                (first_pos, second_pos),
                (first_pos + offset_amount, second_pos + offset_amount),
            ],
            first_pos.min(second_pos),
            true,
        )?;

        let origin = BlockPos::zero();

        let clipboard = create_clipboard(ctx.world_mut(), origin, first_pos, second_pos);
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

        ctx.worldedit_message(&format!(
            "Your selection was moved. ({:?})",
            start_time.elapsed()
        ))
    }

    let move_flags_arg = ArgumentType::flags()
        .add('a', "ignore-air", "Move without air blocks")
        .add('s', "shift-selection", "Shift selection with the move");

    registry.register(
        CommandNode::literal("/move")
            .require_permission("worldedit.region.move")
            .require_plot_ownership()
            .mutates_world()
            .then(
                CommandNode::argument("count", ArgumentType::integer(1, 1000))
                    .then(
                        CommandNode::argument("direction", ArgumentType::direction()).then(
                            CommandNode::argument("flags", move_flags_arg.clone()).executes(
                                |ctx| {
                                    let count = ctx.args().get_integer("count")?;
                                    let direction = ctx.args().get_direction("direction")?;
                                    exec_move(ctx, count, direction)
                                },
                            ),
                        ),
                    )
                    .then(
                        CommandNode::argument("flags", move_flags_arg.clone()).executes(|ctx| {
                            let count = ctx.args().get_integer("count")?;
                            exec_move(ctx, count, Direction::Me)
                        }),
                    ),
            ),
    );

    enum ExpandMode {
        Vertical,
        Directional {
            amount: i32,
            reverse_amount: Option<i32>,
            direction: Direction,
        },
    }

    fn exec_expand(ctx: &mut ExecutionContext<'_>, mode: ExpandMode) -> CommandResult<()> {
        let (first, second) = ctx.get_selection()?;

        let (new_first, new_second) = match mode {
            ExpandMode::Vertical => {
                let mut new_first = first;
                let mut new_second = second;

                new_first.y = new_first.y.min(0);
                new_second.y = new_second.y.max(PLOT_BLOCK_HEIGHT);

                (new_first, new_second)
            }
            ExpandMode::Directional {
                amount,
                reverse_amount,
                direction,
            } => {
                let player = ctx.player()?;
                let direction = direction.resolve(player.get_facing());

                let offset = direction.offset_pos(BlockPos::zero(), amount);
                let (mut new_first, mut new_second) =
                    calculate_expanded_selection(first, second, offset, false);

                if let Some(reverse_amt) = reverse_amount {
                    let reverse_offset = direction.offset_pos(BlockPos::zero(), -reverse_amt);
                    (new_first, new_second) =
                        calculate_expanded_selection(new_first, new_second, reverse_offset, false);
                }

                (new_first, new_second)
            }
        };

        let total = calculate_selection_volume(new_first, new_second)
            - calculate_selection_volume(first, second);

        let player = ctx.player_mut()?;
        if new_first != first {
            player.worldedit_set_first_pos(new_first);
        }
        if new_second != second {
            player.worldedit_set_second_pos(new_second);
        }

        ctx.worldedit_message(&format!("Region expanded {} block(s).", total))
    }

    registry.register(
        CommandNode::literal("/expand")
            .alias("/e")
            .require_permission("worldedit.selection.expand")
            .require_plot_ownership()
            .then(
                CommandNode::literal("vert").executes(|ctx| exec_expand(ctx, ExpandMode::Vertical)),
            )
            .then(
                CommandNode::argument("amount", ArgumentType::integer(1, 1000))
                    .executes(|ctx| {
                        let amount = ctx.args().get_integer("amount")?;
                        exec_expand(
                            ctx,
                            ExpandMode::Directional {
                                amount,
                                reverse_amount: None,
                                direction: Direction::Me,
                            },
                        )
                    })
                    .then(
                        CommandNode::argument("reverseAmount", ArgumentType::integer(1, 1000))
                            .executes(|ctx| {
                                let amount = ctx.args().get_integer("amount")?;
                                let reverse_amount = ctx.args().get_integer("reverseAmount")?;
                                exec_expand(
                                    ctx,
                                    ExpandMode::Directional {
                                        amount,
                                        reverse_amount: Some(reverse_amount),
                                        direction: Direction::Me,
                                    },
                                )
                            })
                            .then(
                                CommandNode::argument("direction", ArgumentType::direction())
                                    .executes(|ctx| {
                                        let amount = ctx.args().get_integer("amount")?;
                                        let reverse_amount =
                                            ctx.args().get_integer("reverseAmount")?;
                                        let direction = ctx.args().get_direction("direction")?;
                                        exec_expand(
                                            ctx,
                                            ExpandMode::Directional {
                                                amount,
                                                reverse_amount: Some(reverse_amount),
                                                direction,
                                            },
                                        )
                                    }),
                            ),
                    )
                    .then(
                        CommandNode::argument("direction", ArgumentType::direction()).executes(
                            |ctx| {
                                let amount = ctx.args().get_integer("amount")?;
                                let direction = ctx.args().get_direction("direction")?;
                                exec_expand(
                                    ctx,
                                    ExpandMode::Directional {
                                        amount,
                                        reverse_amount: None,
                                        direction,
                                    },
                                )
                            },
                        ),
                    ),
            ),
    );

    fn exec_contract(
        ctx: &mut ExecutionContext<'_>,
        amount: i32,
        reverse_amount: Option<i32>,
        direction: Direction,
    ) -> CommandResult<()> {
        let (first, second) = ctx.get_selection()?;

        let player = ctx.player_mut()?;
        let direction = direction.resolve(player.get_facing());

        let offset = direction.offset_pos(BlockPos::zero(), amount);
        let (mut new_first, mut new_second) =
            calculate_expanded_selection(first, second, offset, true);

        if let Some(reverse_amt) = reverse_amount {
            let reverse_offset = direction.offset_pos(BlockPos::zero(), -reverse_amt);
            (new_first, new_second) =
                calculate_expanded_selection(new_first, new_second, reverse_offset, true);
        }

        let total = calculate_selection_volume(first, second)
            - calculate_selection_volume(new_first, new_second);

        if new_first != first {
            player.worldedit_set_first_pos(new_first);
        }
        if new_second != second {
            player.worldedit_set_second_pos(new_second);
        }

        ctx.worldedit_message(&format!("Region contracted {} block(s).", total))
    }

    registry.register(
        CommandNode::literal("/contract")
            .require_permission("worldedit.selection.contract")
            .require_plot_ownership()
            .then(
                CommandNode::argument("amount", ArgumentType::integer(1, 1000))
                    .executes(|ctx| {
                        let amount = ctx.args().get_integer("amount")?;
                        exec_contract(ctx, amount, None, Direction::Me)
                    })
                    .then(
                        CommandNode::argument("reverseAmount", ArgumentType::integer(1, 1000))
                            .executes(|ctx| {
                                let amount = ctx.args().get_integer("amount")?;
                                let reverse_amount = ctx.args().get_integer("reverseAmount")?;
                                exec_contract(ctx, amount, Some(reverse_amount), Direction::Me)
                            })
                            .then(
                                CommandNode::argument("direction", ArgumentType::direction())
                                    .executes(|ctx| {
                                        let amount = ctx.args().get_integer("amount")?;
                                        let reverse_amount =
                                            ctx.args().get_integer("reverseAmount")?;
                                        let direction = ctx.args().get_direction("direction")?;
                                        exec_contract(ctx, amount, Some(reverse_amount), direction)
                                    }),
                            ),
                    )
                    .then(
                        CommandNode::argument("direction", ArgumentType::direction()).executes(
                            |ctx| {
                                let amount = ctx.args().get_integer("amount")?;
                                let direction = ctx.args().get_direction("direction")?;
                                exec_contract(ctx, amount, None, direction)
                            },
                        ),
                    ),
            ),
    );

    fn exec_shift(
        ctx: &mut ExecutionContext<'_>,
        amount: i32,
        direction: Direction,
    ) -> CommandResult<()> {
        let player = ctx.player()?;
        let direction = direction.resolve(player.get_facing());

        let (first, second) = ctx.get_selection()?;
        let offset = direction.offset_pos(BlockPos::zero(), amount);

        let player = ctx.player_mut()?;
        player.worldedit_set_first_pos(first + offset);
        player.worldedit_set_second_pos(second + offset);
        ctx.worldedit_message(&format!("Region shifted {} block(s).", amount))?;
        Ok(())
    }

    registry.register(
        CommandNode::literal("/shift")
            .require_permission("worldedit.selection.shift")
            .require_plot_ownership()
            .then(
                CommandNode::argument("amount", ArgumentType::integer(1, 1000))
                    .executes(|ctx| {
                        let amount = ctx.args().get_integer("amount")?;
                        exec_shift(ctx, amount, Direction::Me)
                    })
                    .then(
                        CommandNode::argument("direction", ArgumentType::direction()).executes(
                            |ctx| {
                                let amount = ctx.args().get_integer("amount")?;
                                let direction = ctx.args().get_direction("direction")?;
                                exec_shift(ctx, amount, direction)
                            },
                        ),
                    ),
            ),
    );

    fn exec_flip(ctx: &mut ExecutionContext<'_>, direction: Direction) -> CommandResult<()> {
        let player = ctx.player()?;
        let direction = direction.resolve(player.get_facing());

        let start_time = Instant::now();
        let clipboard = ctx.get_clipboard()?.clone();
        let size_x = clipboard.size_x;
        let size_y = clipboard.size_y;
        let size_z = clipboard.size_z;
        let volume = size_x * size_y * size_z;

        let flip_pos = |mut pos: BlockPos| {
            match direction {
                BlockFacing::East | BlockFacing::West => pos.x = size_x as i32 - 1 - pos.x,
                BlockFacing::North | BlockFacing::South => pos.z = size_z as i32 - 1 - pos.z,
                BlockFacing::Up | BlockFacing::Down => pos.y = size_y as i32 - 1 - pos.y,
            }
            pos
        };

        let mut newcpdata = PalettedBitBuffer::new(volume as usize, 9);

        let mut c_x = 0;
        let mut c_y = 0;
        let mut c_z = 0;
        for i in 0..volume {
            let BlockPos {
                x: n_x,
                y: n_y,
                z: n_z,
            } = flip_pos(BlockPos::new(c_x, c_y, c_z));
            let n_i = (n_y as u32 * size_x * size_z) + (n_z as u32 * size_x) + n_x as u32;

            let mut block = Block::from_id(clipboard.data.get_entry(i as usize));
            match direction {
                BlockFacing::East | BlockFacing::West => block.flip(FlipDirection::FlipX),
                BlockFacing::North | BlockFacing::South => block.flip(FlipDirection::FlipZ),
                _ => {}
            }
            newcpdata.set_entry(n_i as usize, block.get_id());

            c_x += 1;
            if c_x as u32 == size_x {
                c_x = 0;
                c_z += 1;
                if c_z as u32 == size_z {
                    c_z = 0;
                    c_y += 1;
                }
            }
        }

        let offset = flip_pos(BlockPos::new(
            clipboard.offset_x,
            clipboard.offset_y,
            clipboard.offset_z,
        ));
        let cb = WorldEditClipboard {
            offset_x: offset.x,
            offset_y: offset.y,
            offset_z: offset.z,
            size_x,
            size_y,
            size_z,
            data: newcpdata,
            block_entities: clipboard
                .block_entities
                .iter()
                .map(|(pos, e)| (flip_pos(*pos), e.clone()))
                .collect(),
        };

        ctx.set_clipboard(cb)?;
        ctx.worldedit_message(&format!(
            "The clipboard copy has been flipped. ({:?})",
            start_time.elapsed()
        ))
    }

    registry.register(
        CommandNode::literal("/flip")
            .alias("/f")
            .require_permission("worldedit.clipboard.flip")
            .require_plot_ownership()
            .executes(|ctx| exec_flip(ctx, Direction::Me))
            .then(
                CommandNode::argument("direction", ArgumentType::direction()).executes(|ctx| {
                    let direction = ctx.args().get_direction("direction")?;
                    exec_flip(ctx, direction)
                }),
            ),
    );

    registry.register(
        CommandNode::literal("/rotate")
            .alias("/r")
            .require_permission("worldedit.clipboard.rotate")
            .require_plot_ownership()
            .then(
                CommandNode::argument("angle", ArgumentType::integer(-360, 360)).executes(|ctx| {
                    let angle = ctx.args().get_integer("angle")?;

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

                    let clipboard = ctx.get_clipboard()?.clone();
                    let size_x = clipboard.size_x;
                    let size_y = clipboard.size_y;
                    let size_z = clipboard.size_z;
                    let volume = size_x * size_y * size_z;

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

                    let mut newcpdata = PalettedBitBuffer::new(volume as usize, 9);

                    let mut c_x = 0;
                    let mut c_y = 0;
                    let mut c_z = 0;
                    for i in 0..volume {
                        let BlockPos {
                            x: n_x,
                            y: n_y,
                            z: n_z,
                        } = rotate_pos(BlockPos::new(c_x, c_y, c_z));
                        let n_i = (n_y as u32 * n_size_x * n_size_z)
                            + (n_z as u32 * n_size_x)
                            + n_x as u32;

                        let mut block = Block::from_id(clipboard.data.get_entry(i as usize));
                        block.rotate(rotate_amt);
                        newcpdata.set_entry(n_i as usize, block.get_id());

                        c_x += 1;
                        if c_x as u32 == size_x {
                            c_x = 0;
                            c_z += 1;
                            if c_z as u32 == size_z {
                                c_z = 0;
                                c_y += 1;
                            }
                        }
                    }

                    let offset = rotate_pos(BlockPos::new(
                        clipboard.offset_x,
                        clipboard.offset_y,
                        clipboard.offset_z,
                    ));
                    let cb = WorldEditClipboard {
                        offset_x: offset.x,
                        offset_y: offset.y,
                        offset_z: offset.z,
                        size_x: n_size_x,
                        size_y,
                        size_z: n_size_z,
                        data: newcpdata,
                        block_entities: clipboard
                            .block_entities
                            .iter()
                            .map(|(pos, e)| (rotate_pos(*pos), e.clone()))
                            .collect(),
                    };

                    ctx.set_clipboard(cb)?;
                    ctx.worldedit_message(&format!(
                        "The clipboard copy has been rotated. ({:?})",
                        start_time.elapsed()
                    ))
                }),
            ),
    );

    fn exec_schematic_list(ctx: &mut ExecutionContext<'_>) -> CommandResult<()> {
        let player = ctx.player()?;

        let schems_dir = if CONFIG.schemati {
            let uuid = HyphenatedUUID(player.uuid).to_string();
            format!("./schems/{}", uuid)
        } else {
            "./schems".to_string()
        };

        let entries = match std::fs::read_dir(&schems_dir) {
            Ok(entries) => entries,
            Err(_) => {
                return ctx.worldedit_message("No schematics found.");
            }
        };

        let mut schematics = Vec::new();
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_file() {
                    if let Some(filename) = entry.file_name().to_str() {
                        if filename.ends_with(".schem") || filename.ends_with(".schematic") {
                            schematics.push(filename.to_string());
                        }
                    }
                }
            }
        }

        if schematics.is_empty() {
            return ctx.worldedit_message("No schematics found.");
        }

        schematics.sort();
        let list = schematics.join(", ");
        ctx.worldedit_message(&format!(
            "Available schematics ({}): {}",
            schematics.len(),
            list
        ))
    }

    fn exec_schematic_load(ctx: &mut ExecutionContext<'_>, filename: String) -> CommandResult<()> {
        let start_time = Instant::now();
        let mut filename = filename;

        if !SCHEMATI_VALIDATE_REGEX.is_match(&filename) {
            return Err(CommandError::runtime("Filename is invalid"));
        }

        if CONFIG.schemati {
            let player = ctx.player()?;
            let prefix = HyphenatedUUID(player.uuid).to_string() + "/";
            filename.insert_str(0, &prefix);
        }

        match load_schematic(&filename) {
            Ok(clipboard) => {
                ctx.set_clipboard(clipboard)?;
                ctx.worldedit_message(&format!(
                    "The schematic was loaded to your clipboard. Do //paste to birth it into the world. ({:?})",
                    start_time.elapsed()
                ))
            }
            Err(e) => {
                if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
                    if io_err.kind() == std::io::ErrorKind::NotFound {
                        return Err(CommandError::runtime(
                            "The specified schematic file could not be found.",
                        ));
                    }
                }
                warn!("There was an error loading a schematic:");
                warn!("{}", e);
                Err(CommandError::runtime(
                    "There was an error loading the schematic. Check console for more details.",
                ))
            }
        }
    }

    fn exec_schematic_save(ctx: &mut ExecutionContext<'_>, filename: String) -> CommandResult<()> {
        let start_time = Instant::now();
        let mut filename = filename;
        let flags = ctx.args().get_flags("flags").unwrap_or_default();
        let force_overwrite = flags.contains("force-overwrite");

        if !SCHEMATI_VALIDATE_REGEX.is_match(&filename) {
            return Err(CommandError::runtime("Filename is invalid"));
        }

        if CONFIG.schemati {
            let player = ctx.player()?;
            let uuid = HyphenatedUUID(player.uuid);
            filename = format!("{uuid}/{filename}");
        }

        let path = format!("./schems/{}", filename);
        if !force_overwrite && std::path::Path::new(&path).exists() {
            return Err(CommandError::runtime(
                "File already exists. Use -f flag to overwrite.",
            ));
        }

        let clipboard = ctx.get_clipboard()?.clone();

        match save_schematic(&filename, &clipboard) {
            Ok(_) => ctx.worldedit_message(&format!(
                "The schematic was saved sucessfuly. ({:?})",
                start_time.elapsed()
            )),
            Err(e) => {
                warn!("There was an error saving a schematic:");
                warn!("{:?}", e);
                Err(CommandError::runtime(
                    "There was an error saving the schematic.",
                ))
            }
        }
    }

    registry.register(
        CommandNode::literal("schematic")
            .alias("schem")
            .alias("/schematic")
            .alias("/schem")
            .require_permission("worldedit.schematic")
            .then(
                CommandNode::literal("list")
                    .alias("all")
                    .alias("ls")
                    .executes(exec_schematic_list),
            )
            .then(
                CommandNode::literal("load")
                    .require_permission("worldedit.clipboard.load")
                    .require_plot_ownership()
                    .then(
                        CommandNode::argument("filename", ArgumentType::string()).executes(|ctx| {
                            let filename = ctx.args().get_string("filename")?;
                            exec_schematic_load(ctx, filename)
                        }),
                    ),
            )
            .then(
                CommandNode::literal("save")
                    .require_permission("worldedit.clipboard.save")
                    .require_plot_ownership()
                    .then(
                        CommandNode::argument("filename", ArgumentType::string()).then(
                            CommandNode::argument(
                                "flags",
                                ArgumentType::flags().add(
                                    'f',
                                    "force-overwrite",
                                    "Overwrite existing file",
                                ),
                            )
                            .executes(|ctx| {
                                let filename = ctx.args().get_string("filename")?;
                                exec_schematic_save(ctx, filename)
                            }),
                        ),
                    ),
            ),
    );
    registry.add_custom_alias("/load", "/schematic load");
    registry.add_custom_alias("/save", "/schematic save");

    fn exec_update(ctx: &mut ExecutionContext<'_>) -> CommandResult<()> {
        let flags = ctx.args().get_flags("flags")?;

        if flags.contains("plot") {
            let corners = ctx.world().get_corners();
            update(ctx.world_mut(), corners.0, corners.1);
            ctx.worldedit_message("Updated entire plot.")
        } else {
            let (first_pos, second_pos) = ctx.get_selection()?;
            update(ctx.world_mut(), first_pos, second_pos);
            ctx.worldedit_message("Updated selection.")
        }
    }

    registry.register(
        CommandNode::literal("/update")
            .require_permission("mchprs.we.update")
            .require_plot_ownership()
            .mutates_world()
            .then(
                CommandNode::argument(
                    "flags",
                    ArgumentType::flags().add('p', "plot", "Update the entire plot"),
                )
                .executes(exec_update),
            ),
    );

    registry.register(
        CommandNode::literal("/wand")
            .require_permission("worldedit.wand")
            .require_plot_ownership()
            .executes(|ctx| {
                let item = ItemStack {
                    count: 1,
                    item_type: Item::WEWand {},
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

    fn exec_rstack(
        ctx: &mut ExecutionContext<'_>,
        count: i32,
        spacing: i32,
        direction: DirectionExt,
    ) -> CommandResult<()> {
        let flags = ctx.args().get_flags("flags")?;
        let with_air = flags.contains("with-air");
        let expand_selection_flag = flags.contains("expand-selection");

        let player = ctx.player()?;
        let direction = direction.resolve(player.get_facing(), player.pitch);

        let (first_pos, second_pos) = ctx.get_selection()?;
        let start_time = Instant::now();

        let clipboard = create_clipboard(ctx.world_mut(), first_pos, first_pos, second_pos);

        let undo_positions = (1..=count).rev().map(|i| {
            let offset = direction * (i * spacing);
            (first_pos + offset, second_pos + offset)
        });
        ctx.capture_undo_regions(undo_positions, first_pos, true)?;

        for i in 1..=count {
            let offset = direction * (i * spacing);
            let paste_pos = first_pos + offset;
            paste_clipboard(ctx.world_mut(), &clipboard, paste_pos, !with_air);
        }

        if expand_selection_flag {
            let offset = direction * (count * spacing);
            let (new_first, new_second) =
                calculate_expanded_selection(first_pos, second_pos, offset, false);
            let player = ctx.player_mut()?;
            if new_first != first_pos {
                player.worldedit_set_first_pos(new_first);
            }
            if new_second != second_pos {
                player.worldedit_set_second_pos(new_second);
            }
        }

        ctx.worldedit_message(&format!(
            "Your selection was stacked successfully. ({:?})",
            start_time.elapsed()
        ))
    }

    let rstack_flags_arg = ArgumentType::flags()
        .add('w', "with-air", "Stack with air blocks")
        .add(
            'e',
            "expand-selection",
            "Expand selection to include stacked region",
        );

    registry.register(
        CommandNode::literal("/rstack")
            .alias("/rs")
            .require_permission("redstonetools.rstack")
            .require_plot_ownership()
            .mutates_world()
            .then(
                CommandNode::argument("count", ArgumentType::integer(1, 1000))
                    .then(
                        CommandNode::argument("spacing", ArgumentType::integer(1, 1000))
                            .then(
                                CommandNode::argument(
                                    "direction-ext",
                                    ArgumentType::direction_ext(),
                                )
                                .then(
                                    CommandNode::argument("flags", rstack_flags_arg.clone())
                                        .executes(|ctx| {
                                            let count = ctx.args().get_integer("count")?;
                                            let spacing = ctx.args().get_integer("spacing")?;
                                            let direction =
                                                ctx.args().get_direction_ext("direction-ext")?;
                                            exec_rstack(ctx, count, spacing, direction)
                                        }),
                                ),
                            )
                            .then(
                                CommandNode::argument("flags", rstack_flags_arg.clone()).executes(
                                    |ctx| {
                                        let count = ctx.args().get_integer("count")?;
                                        let spacing = ctx.args().get_integer("spacing")?;
                                        exec_rstack(ctx, count, spacing, DirectionExt::Me)
                                    },
                                ),
                            ),
                    )
                    .then(
                        CommandNode::argument("direction-ext", ArgumentType::direction_ext())
                            .then(
                                CommandNode::argument("spacing", ArgumentType::integer(1, 1000))
                                    .then(
                                        CommandNode::argument("flags", rstack_flags_arg.clone())
                                            .executes(|ctx| {
                                                let count = ctx.args().get_integer("count")?;
                                                let spacing = ctx.args().get_integer("spacing")?;
                                                let direction = ctx
                                                    .args()
                                                    .get_direction_ext("direction-ext")?;
                                                exec_rstack(ctx, count, spacing, direction)
                                            }),
                                    ),
                            )
                            .then(
                                CommandNode::argument("flags", rstack_flags_arg.clone()).executes(
                                    |ctx| {
                                        let count = ctx.args().get_integer("count")?;
                                        let direction =
                                            ctx.args().get_direction_ext("direction-ext")?;
                                        exec_rstack(ctx, count, 2, direction)
                                    },
                                ),
                            ),
                    )
                    .then(
                        CommandNode::argument("flags", rstack_flags_arg.clone()).executes(|ctx| {
                            let count = ctx.args().get_integer("count")?;
                            exec_rstack(ctx, count, 2, DirectionExt::Me)
                        }),
                    ),
            ),
    );

    fn exec_replacecontainer(
        ctx: &mut ExecutionContext<'_>,
        from: Option<ContainerType>,
        to: ContainerType,
    ) -> CommandResult<()> {
        let (first_pos, second_pos) = ctx.get_selection()?;

        let start_time = Instant::now();

        let new_block = match to {
            ContainerType::Furnace => Block::Furnace {},
            ContainerType::Barrel => Block::Barrel {},
            ContainerType::Hopper => Block::Hopper {},
        };

        let start_pos = first_pos.min(second_pos);
        let end_pos = first_pos.max(second_pos);

        for x in start_pos.x..=end_pos.x {
            for y in start_pos.y..=end_pos.y {
                for z in start_pos.z..=end_pos.z {
                    let pos = BlockPos::new(x, y, z);
                    let block = ctx.world().get_block(pos);

                    if !matches!(
                        block,
                        Block::Furnace {} | Block::Barrel {} | Block::Hopper {}
                    ) {
                        continue;
                    }

                    let block_entity = ctx.world().get_block_entity(pos);
                    if let Some(BlockEntity::Container {
                        comparator_override,
                        ty,
                        ..
                    }) = block_entity
                    {
                        let should_replace = from.is_none_or(|from| from == *ty);

                        if !should_replace {
                            continue;
                        }

                        let ss = *comparator_override;

                        let items_needed = to.items_needed_for_signal_strength(ss);

                        let mut inventory = Vec::new();
                        for (slot, items_added) in (0..items_needed).step_by(64).enumerate() {
                            let count = (items_needed - items_added).min(64);
                            inventory.push(InventoryEntry {
                                id: Item::Redstone {}.get_id(),
                                slot: slot as i8,
                                count: count as i8,
                                nbt: None,
                            });
                        }

                        let new_entity = BlockEntity::Container {
                            comparator_override: ss,
                            inventory,
                            ty: to,
                        };
                        ctx.world_mut().set_block_entity(pos, new_entity);
                        ctx.world_mut().set_block(pos, new_block);
                    }
                }
            }
        }

        ctx.worldedit_message(&format!(
            "Your selection was replaced successfully. ({:?})",
            start_time.elapsed()
        ))
    }

    registry.register(
        CommandNode::literal("/replacecontainer")
            .alias("/rc")
            .require_permission("mchprs.we.replacecontainer")
            .require_plot_ownership()
            .mutates_world()
            .then(
                CommandNode::argument("to", ArgumentType::container()).executes(|ctx| {
                    let to = ctx.args().get_container("to")?;
                    exec_replacecontainer(ctx, None, to)
                }),
            )
            .then(
                CommandNode::argument("from", ArgumentType::container()).then(
                    CommandNode::argument("to", ArgumentType::container()).executes(|ctx| {
                        let from = ctx.args().get_container("from")?;
                        let to = ctx.args().get_container("to")?;
                        exec_replacecontainer(ctx, Some(from), to)
                    }),
                ),
            ),
    );
}
