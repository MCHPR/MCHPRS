use crate::commands::error::{CommandError, CommandResult, RuntimeError};
use crate::commands::value::DirectionExt;
use crate::commands::{
    argument::ArgumentType, context::ExecutionContext, node::CommandNode, registry::CommandRegistry,
};
use crate::config::CONFIG;
use crate::plot::PLOT_BLOCK_HEIGHT;
use crate::utils::{self, HyphenatedUUID};
use crate::worldedit::schematic::{load_schematic, save_schematic};
use crate::worldedit::{
    clear_area, create_clipboard, expand_selection, paste_clipboard, ray_trace_block, update,
    WorldEditClipboard, WorldEditUndo,
};
use mchprs_blocks::block_entities::{BlockEntity, ContainerType, InventoryEntry};
use mchprs_blocks::blocks::{Block, FlipDirection, RotateAmt};
use mchprs_blocks::items::{Item, ItemStack};
use mchprs_blocks::{BlockFacing, BlockPos};
use mchprs_network::packets::clientbound::*;
use mchprs_world::storage::PalettedBitBuffer;
use mchprs_world::World;
use once_cell::sync::Lazy;
use regex::Regex;
use std::time::Instant;
use tracing::warn;

static SCHEMATI_VALIDATE_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[a-zA-Z0-9_.]+\.schem(atic)?").unwrap());

pub(super) fn register_commands(registry: &mut CommandRegistry) {
    fn exec_up(ctx: &mut ExecutionContext<'_>, distance: i32) -> CommandResult<()> {
        let player_pos = ctx.player()?.pos;

        let mut new_pos = player_pos;
        new_pos.y += distance as f64;
        let block_pos = new_pos.block_pos();

        // Place glass platform below player if there's air
        let platform_pos = BlockPos::new(block_pos.x, block_pos.y - 1, block_pos.z);
        if matches!(ctx.world().get_block(platform_pos), Block::Air {}) {
            ctx.world_mut().set_block(platform_pos, Block::Glass {});
        }

        ctx.player_mut()?.teleport(new_pos);
        ctx.worldedit_message(&format!("Moved up {} blocks", distance))?;
        Ok(())
    }

    registry.register(
        CommandNode::literal("up")
            .alias("u")
            .require_permission("worldedit.navigation.up")
            .mutates_world()
            .executes(|ctx| exec_up(ctx, 1))
            .then(
                CommandNode::argument("distance", ArgumentType::integer(1, 100)).executes(|ctx| {
                    let distance = ctx.args().get_integer("distance")?;
                    exec_up(ctx, distance)
                }),
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
                ctx.player_mut()?.worldedit_set_first_position(pos);
                Ok(())
            }),
    );

    registry.register(
        CommandNode::literal("/pos2")
            .alias("/2")
            .require_permission("worldedit.selection.pos")
            .require_plot_ownership()
            .executes(|ctx| {
                let pos = ctx.player()?.pos.block_pos();
                ctx.player_mut()?.worldedit_set_second_position(pos);
                Ok(())
            }),
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
                let pos = match ray_trace_block(&ctx.plot.world, player_pos, pitch, yaw, 300.0) {
                    Some(pos) => pos,
                    None => return Err(RuntimeError::NoBlockInSight.into()),
                };
                ctx.player_mut()?.worldedit_set_first_position(pos);
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
                let pos = match ray_trace_block(&ctx.plot.world, player_pos, pitch, yaw, 300.0) {
                    Some(pos) => pos,
                    None => return Err(RuntimeError::NoBlockInSight.into()),
                };
                ctx.player_mut()?.worldedit_set_second_position(pos);
                Ok(())
            }),
    );

    registry.register(
        CommandNode::literal("/sel")
            .require_permission("worldedit.selection.pos")
            .require_plot_ownership()
            .executes(|ctx| {
                let player = ctx.player_mut()?;
                player.first_position = None;
                player.second_position = None;
                player.send_worldedit_message("Selection cleared.");
                player.worldedit_send_cui("s|cuboid");
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

    registry.register(
        CommandNode::literal("/replace")
            .require_permission("worldedit.region.replace")
            .require_plot_ownership()
            .mutates_world()
            .then(CommandNode::argument("from", ArgumentType::mask()).then(
                CommandNode::argument("to", ArgumentType::pattern()).executes(|ctx| {
                    let mask = ctx.args().get_mask("from")?;
                    let pattern = ctx.args().get_pattern("to")?;
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

                                if mask.matches(ctx.world().get_block(block_pos)) {
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

    registry.register(
        CommandNode::literal("/copy")
            .alias("/c")
            .require_permission("worldedit.clipboard.copy")
            .require_plot_ownership()
            .executes(|ctx| {
                let (first_pos, second_pos) = ctx.get_selection()?;

                let start_time = Instant::now();
                let origin = ctx.player()?.pos.block_pos();
                let clipboard =
                    create_clipboard(&mut ctx.plot.world, origin, first_pos, second_pos);
                ctx.set_clipboard(clipboard)?;

                ctx.worldedit_message(&format!(
                    "Your selection was copied. ({:?})",
                    start_time.elapsed()
                ))
            }),
    );

    registry.register(
        CommandNode::literal("/cut")
            .alias("/x")
            .require_permission("worldedit.clipboard.cut")
            .require_plot_ownership()
            .mutates_world()
            .executes(|ctx| {
                let (first_pos, second_pos) = ctx.get_selection()?;

                let origin = first_pos.min(second_pos);
                ctx.capture_undo_regions([(first_pos, second_pos)], origin, true)?;

                let start_time = Instant::now();
                let origin = ctx.player()?.pos.block_pos();
                let clipboard =
                    create_clipboard(&mut ctx.plot.world, origin, first_pos, second_pos);
                ctx.set_clipboard(clipboard)?;
                clear_area(&mut ctx.plot.world, first_pos, second_pos);

                ctx.worldedit_message(&format!(
                    "Your selection was cut. ({:?})",
                    start_time.elapsed()
                ))
            }),
    );

    fn exec_paste(ctx: &mut ExecutionContext<'_>) -> CommandResult<()> {
        let clipboard = ctx.get_clipboard()?.clone();

        let flags = ctx.args().get_flags("flags")?;
        let ignore_air = flags.contains("ignore-air");
        let should_update = flags.contains("update");

        let start_time = Instant::now();
        let pos = ctx.player()?.pos.block_pos();

        let paste_min = BlockPos::new(
            pos.x - clipboard.offset_x,
            pos.y - clipboard.offset_y,
            pos.z - clipboard.offset_z,
        );
        let paste_max = BlockPos::new(
            paste_min.x + clipboard.size_x as i32 - 1,
            paste_min.y + clipboard.size_y as i32 - 1,
            paste_min.z + clipboard.size_z as i32 - 1,
        );
        ctx.capture_undo_regions([(paste_min, paste_max)], paste_min, true)?;

        paste_clipboard(&mut ctx.plot.world, &clipboard, pos, ignore_air);

        if should_update {
            update(&mut ctx.plot.world, paste_min, paste_max);
        }

        ctx.worldedit_message(&format!(
            "Your clipboard was pasted. ({:?})",
            start_time.elapsed()
        ))
    }

    registry.register(
        CommandNode::literal("/paste")
            .alias("/v")
            .require_permission("worldedit.clipboard.paste")
            .require_plot_ownership()
            .mutates_world()
            .then(
                CommandNode::argument(
                    "flags",
                    ArgumentType::flags()
                        .add(("ignore-air", 'a'))
                        .add(("update", 'u')),
                )
                .executes(exec_paste),
            ),
    );
    registry.add_custom_alias("/va", "/paste -a");

    registry.register(
        CommandNode::literal("/undo")
            .require_permission("worldedit.history.undo")
            .require_plot_ownership()
            .mutates_world()
            .executes(|ctx| {
                let player = ctx.player_mut()?;

                if let Some(undo) = player.worldedit_undo.pop() {
                    let plot_x = ctx.plot.world.x;
                    let plot_z = ctx.plot.world.z;

                    if undo.plot_x != plot_x || undo.plot_z != plot_z {
                        return Err(RuntimeError::UndoFromDifferentPlot.into());
                    }

                    let redo_clipboards: Vec<WorldEditClipboard> = undo
                        .clipboards
                        .iter()
                        .map(|cb| {
                            let first_pos = BlockPos {
                                x: undo.pos.x - cb.offset_x,
                                y: undo.pos.y - cb.offset_y,
                                z: undo.pos.z - cb.offset_z,
                            };
                            let second_pos = BlockPos {
                                x: first_pos.x + cb.size_x as i32 - 1,
                                y: first_pos.y + cb.size_y as i32 - 1,
                                z: first_pos.z + cb.size_z as i32 - 1,
                            };
                            create_clipboard(&mut ctx.plot.world, undo.pos, first_pos, second_pos)
                        })
                        .collect();

                    for clipboard in &undo.clipboards {
                        paste_clipboard(&mut ctx.plot.world, clipboard, undo.pos, false);
                    }

                    let redo = WorldEditUndo {
                        clipboards: redo_clipboards,
                        pos: undo.pos,
                        plot_x,
                        plot_z,
                    };
                    ctx.player_mut()?.worldedit_redo.push(redo);

                    ctx.worldedit_message("Undo successful.")
                } else {
                    Err(RuntimeError::NoUndoHistory.into())
                }?;
                Ok(())
            }),
    );

    registry.register(
        CommandNode::literal("/redo")
            .require_permission("worldedit.history.redo")
            .require_plot_ownership()
            .mutates_world()
            .executes(|ctx| {
                let player = ctx.player_mut()?;

                if let Some(redo) = player.worldedit_redo.pop() {
                    let plot_x = ctx.plot.world.x;
                    let plot_z = ctx.plot.world.z;

                    if redo.plot_x != plot_x || redo.plot_z != plot_z {
                        return Err(RuntimeError::RedoFromDifferentPlot.into());
                    }

                    let undo_clipboards: Vec<WorldEditClipboard> = redo
                        .clipboards
                        .iter()
                        .map(|cb| {
                            let first_pos = BlockPos {
                                x: redo.pos.x - cb.offset_x,
                                y: redo.pos.y - cb.offset_y,
                                z: redo.pos.z - cb.offset_z,
                            };
                            let second_pos = BlockPos {
                                x: first_pos.x + cb.size_x as i32 - 1,
                                y: first_pos.y + cb.size_y as i32 - 1,
                                z: first_pos.z + cb.size_z as i32 - 1,
                            };
                            create_clipboard(&mut ctx.plot.world, redo.pos, first_pos, second_pos)
                        })
                        .collect();

                    for clipboard in &redo.clipboards {
                        paste_clipboard(&mut ctx.plot.world, clipboard, redo.pos, false);
                    }

                    let undo = WorldEditUndo {
                        clipboards: undo_clipboards,
                        pos: redo.pos,
                        plot_x,
                        plot_z,
                    };
                    ctx.player_mut()?.worldedit_undo.push(undo);

                    ctx.worldedit_message("Redo successful.")
                } else {
                    Err(RuntimeError::NoRedoHistory.into())
                }?;
                Ok(())
            }),
    );

    fn exec_stack(
        ctx: &mut ExecutionContext<'_>,
        count: i32,
        direction: BlockFacing,
    ) -> CommandResult<()> {
        let flags = ctx.args().get_flags("flags")?;
        let ignore_air = flags.contains("ignore-air");

        let (first_pos, second_pos) = ctx.get_selection()?;
        let start_time = Instant::now();

        let clipboard = create_clipboard(&mut ctx.plot.world, first_pos, first_pos, second_pos);
        let stack_offset = match direction {
            BlockFacing::North | BlockFacing::South => clipboard.size_z as i32,
            BlockFacing::East | BlockFacing::West => clipboard.size_x as i32,
            BlockFacing::Up | BlockFacing::Down => clipboard.size_y as i32,
        };

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
            paste_clipboard(&mut ctx.plot.world, &clipboard, paste_pos, ignore_air);
        }

        ctx.worldedit_message(&format!(
            "Your selection was stacked. ({:?})",
            start_time.elapsed()
        ))
    }

    let stack_flag_argument = ArgumentType::flags().add(("ignore-air", 'a'));
    registry.register(
        CommandNode::literal("/stack")
            .alias("/s")
            .require_permission("worldedit.region.stack")
            .require_plot_ownership()
            .mutates_world()
            .then(
                CommandNode::argument("count", ArgumentType::integer(1, 1000))
                    .then(
                        CommandNode::argument("direction", ArgumentType::direction()).then(
                            CommandNode::argument("flags", stack_flag_argument.clone()).executes(
                                |ctx| {
                                    let count = ctx.args().get_integer("count")?;
                                    let direction = ctx.args().get_direction("direction")?;
                                    let player_facing = ctx.player()?.get_facing();
                                    let direction = direction.resolve(player_facing);
                                    exec_stack(ctx, count, direction)
                                },
                            ),
                        ),
                    )
                    .then(
                        CommandNode::argument("flags", stack_flag_argument.clone()).executes(
                            |ctx| {
                                let count = ctx.args().get_integer("count")?;
                                let player_facing = ctx.player()?.get_facing();
                                exec_stack(ctx, count, player_facing)
                            },
                        ),
                    ),
            ),
    );
    registry.add_custom_alias("/sa", "/stack {} -a");

    fn exec_move(
        ctx: &mut ExecutionContext<'_>,
        count: i32,
        direction: BlockFacing,
    ) -> CommandResult<()> {
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
        let clipboard = create_clipboard(&mut ctx.plot.world, origin, first_pos, second_pos);
        clear_area(&mut ctx.plot.world, first_pos, second_pos);
        paste_clipboard(
            &mut ctx.plot.world,
            &clipboard,
            origin + offset_amount,
            ignore_air,
        );

        if shift_selection {
            let player = ctx.player_mut()?;
            player.worldedit_set_first_position(first_pos + offset_amount);
            player.worldedit_set_second_position(second_pos + offset_amount);
        }

        ctx.worldedit_message(&format!(
            "Your selection was moved. ({:?})",
            start_time.elapsed()
        ))
    }

    let move_flag_args = ArgumentType::flags()
        .add(("ignore-air", 'a'))
        .add(("shift-selection", 's'));
    registry.register(
        CommandNode::literal("/move")
            .require_permission("worldedit.region.move")
            .require_plot_ownership()
            .mutates_world()
            .then(
                CommandNode::argument("count", ArgumentType::integer(1, 1000))
                    .then(
                        CommandNode::argument("direction", ArgumentType::direction()).then(
                            CommandNode::argument("flags", move_flag_args.clone()).executes(
                                |ctx| {
                                    let count = ctx.args().get_integer("count")?;
                                    let direction = ctx.args().get_direction("direction")?;
                                    let player_facing = ctx.player()?.get_facing();
                                    let direction = direction.resolve(player_facing);
                                    exec_move(ctx, count, direction)
                                },
                            ),
                        ),
                    )
                    .then(
                        CommandNode::argument("flags", move_flag_args.clone()).executes(|ctx| {
                            let count = ctx.args().get_integer("count")?;
                            let player_facing = ctx.player()?.get_facing();
                            exec_move(ctx, count, player_facing)
                        }),
                    ),
            ),
    );

    fn exec_expand(
        ctx: &mut ExecutionContext<'_>,
        amount: i32,
        direction: BlockFacing,
    ) -> CommandResult<()> {
        let offset = direction.offset_pos(BlockPos::zero(), amount);
        let player = ctx.player_mut()?;
        expand_selection(player, offset, false);
        ctx.worldedit_message(&format!("Region expanded {} block(s).", amount))
    }

    registry.register(
        CommandNode::literal("/expand")
            .alias("/e")
            .require_permission("worldedit.selection.expand")
            .require_plot_ownership()
            .then(
                CommandNode::argument("amount", ArgumentType::integer(1, 1000))
                    .executes(|ctx| {
                        let amount = ctx.args().get_integer("amount")?;
                        let player_facing = ctx.player()?.get_facing();
                        exec_expand(ctx, amount, player_facing)
                    })
                    .then(
                        CommandNode::argument("direction", ArgumentType::direction()).executes(
                            |ctx| {
                                let amount = ctx.args().get_integer("amount")?;
                                let direction = ctx.args().get_direction("direction")?;
                                let player_facing = ctx.player()?.get_facing();
                                let direction = direction.resolve(player_facing);
                                exec_expand(ctx, amount, direction)
                            },
                        ),
                    ),
            ),
    );

    fn exec_contract(
        ctx: &mut ExecutionContext<'_>,
        amount: i32,
        direction: BlockFacing,
    ) -> CommandResult<()> {
        let offset = direction.offset_pos(BlockPos::zero(), amount);
        let player = ctx.player_mut()?;
        expand_selection(player, offset, true);
        ctx.worldedit_message(&format!("Region contracted {} block(s).", amount))
    }

    registry.register(
        CommandNode::literal("/contract")
            .require_permission("worldedit.selection.contract")
            .require_plot_ownership()
            .then(
                CommandNode::argument("amount", ArgumentType::integer(1, 1000))
                    .executes(|ctx| {
                        let amount = ctx.args().get_integer("amount")?;
                        let player_facing = ctx.player()?.get_facing();
                        exec_contract(ctx, amount, player_facing)
                    })
                    .then(
                        CommandNode::argument("direction", ArgumentType::direction()).executes(
                            |ctx| {
                                let amount = ctx.args().get_integer("amount")?;
                                let direction = ctx.args().get_direction("direction")?;
                                let player_facing = ctx.player()?.get_facing();
                                let direction = direction.resolve(player_facing);
                                exec_contract(ctx, amount, direction)
                            },
                        ),
                    ),
            ),
    );

    fn exec_shift(
        ctx: &mut ExecutionContext<'_>,
        amount: i32,
        direction: BlockFacing,
    ) -> CommandResult<()> {
        let player = ctx.player_mut()?;
        if let (Some(first), Some(second)) = (player.first_position, player.second_position) {
            let offset = direction.offset_pos(BlockPos::zero(), amount);
            player.worldedit_set_first_position(first + offset);
            player.worldedit_set_second_position(second + offset);
            ctx.worldedit_message(&format!("Region shifted {} block(s).", amount))?;
            Ok(())
        } else {
            Err(RuntimeError::NoSelection.into())
        }
    }

    registry.register(
        CommandNode::literal("/shift")
            .require_permission("worldedit.selection.shift")
            .require_plot_ownership()
            .then(
                CommandNode::argument("amount", ArgumentType::integer(1, 1000))
                    .executes(|ctx| {
                        let amount = ctx.args().get_integer("amount")?;
                        let player_facing = ctx.player()?.get_facing();
                        exec_shift(ctx, amount, player_facing)
                    })
                    .then(
                        CommandNode::argument("direction", ArgumentType::direction()).executes(
                            |ctx| {
                                let amount = ctx.args().get_integer("amount")?;
                                let direction = ctx.args().get_direction("direction")?;
                                let player_facing = ctx.player()?.get_facing();
                                let direction = direction.resolve(player_facing);
                                exec_shift(ctx, amount, direction)
                            },
                        ),
                    ),
            ),
    );

    fn exec_flip(ctx: &mut ExecutionContext<'_>, direction: BlockFacing) -> CommandResult<()> {
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
            .executes(|ctx| {
                let player_facing = ctx.player()?.get_facing();
                exec_flip(ctx, player_facing)
            })
            .then(
                CommandNode::argument("direction", ArgumentType::direction()).executes(|ctx| {
                    let direction = ctx.args().get_direction("direction")?;
                    let player_facing = ctx.player()?.get_facing();
                    let direction = direction.resolve(player_facing);
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

    registry.register(
        CommandNode::literal("/load")
            .require_permission("worldedit.clipboard.load")
            .require_plot_ownership()
            .then(
                CommandNode::argument("filename", ArgumentType::string()).executes(
                    |ctx| {
                                                let start_time = Instant::now();
                        let mut filename = ctx.args().get_string("filename")?;

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
                                        return Err(CommandError::runtime("The specified schematic file could not be found."));
                                    }
                                }
                                warn!("There was an error loading a schematic:");
                                warn!("{}", e);
                                Err(CommandError::runtime("There was an error loading the schematic. Check console for more details."))
                            }
                        }
                    },
                ),
            ),
    );

    registry.register(
        CommandNode::literal("/save")
            .require_permission("worldedit.clipboard.save")
            .require_plot_ownership()
            .then(
                CommandNode::argument("filename", ArgumentType::string()).executes(|ctx| {
                    let start_time = Instant::now();
                    let mut filename = ctx.args().get_string("filename")?;

                    if !SCHEMATI_VALIDATE_REGEX.is_match(&filename) {
                        return Err(CommandError::runtime("Filename is invalid"));
                    }

                    if CONFIG.schemati {
                        let player = ctx.player()?;
                        let prefix = HyphenatedUUID(player.uuid).to_string() + "/";
                        filename.insert_str(0, &prefix);
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
                }),
            ),
    );

    fn exec_update(ctx: &mut ExecutionContext<'_>) -> CommandResult<()> {
        let flags = ctx.args().get_flags("flags")?;

        if flags.contains("plot") {
            let corners = ctx.plot.world.get_corners();
            update(&mut ctx.plot.world, corners.0, corners.1);
            ctx.worldedit_message("Updated entire plot.")
        } else {
            let (first_pos, second_pos) = ctx.get_selection()?;
            update(&mut ctx.plot.world, first_pos, second_pos);
            ctx.worldedit_message("Updated selection.")
        }
    }

    registry.register(
        CommandNode::literal("/update")
            .require_permission("mchprs.we.update")
            .require_plot_ownership()
            .mutates_world()
            .then(
                CommandNode::argument("flags", ArgumentType::flags().add(("plot", 'p')))
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
        let include_air = flags.contains("include-air");
        let expand_selection_flag = flags.contains("expand-selection");

        let player = ctx.player()?;
        let direction = direction.resolve(player.get_facing(), player.pitch);

        let (first_pos, second_pos) = ctx.get_selection()?;
        let start_time = Instant::now();

        let clipboard = create_clipboard(&mut ctx.plot.world, first_pos, first_pos, second_pos);

        let undo_positions = (1..=count).rev().map(|i| {
            let offset = direction * (i * spacing);
            (first_pos + offset, second_pos + offset)
        });
        ctx.capture_undo_regions(undo_positions, first_pos, true)?;

        for i in 1..=count {
            let offset = direction * (i * spacing);
            let paste_pos = first_pos + offset;
            paste_clipboard(&mut ctx.plot.world, &clipboard, paste_pos, !include_air);
        }

        if expand_selection_flag {
            let player = ctx.player_mut()?;
            expand_selection(player, direction * (count * spacing), false);
        }

        ctx.worldedit_message(&format!(
            "Your selection was stacked successfully. ({:?})",
            start_time.elapsed()
        ))
    }

    let rstack_flag_args = ArgumentType::flags()
        .add(("include-air", 'a'))
        .add(("expand-selection", 'e'));
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
                                CommandNode::argument("direction", ArgumentType::direction_ext())
                                    .then(
                                        CommandNode::argument("flags", rstack_flag_args.clone())
                                            .executes(|ctx| {
                                                let count = ctx.args().get_integer("count")?;
                                                let spacing = ctx.args().get_integer("spacing")?;
                                                let direction =
                                                    ctx.args().get_direction_ext("direction")?;
                                                exec_rstack(ctx, count, spacing, direction)
                                            }),
                                    ),
                            )
                            .then(
                                CommandNode::argument("flags", rstack_flag_args.clone()).executes(
                                    |ctx| {
                                        let count = ctx.args().get_integer("count")?;
                                        let spacing = ctx.args().get_integer("spacing")?;
                                        exec_rstack(ctx, count, spacing, DirectionExt::Me)
                                    },
                                ),
                            ),
                    )
                    .then(
                        CommandNode::argument("flags", rstack_flag_args.clone()).executes(|ctx| {
                            let count = ctx.args().get_integer("count")?;
                            exec_rstack(ctx, count, 2, DirectionExt::Me)
                        }),
                    ),
            ),
    );

    registry.register(
        CommandNode::literal("/replacecontainer")
            .alias("/rc")
            .require_permission("mchprs.we.replacecontainer")
            .require_plot_ownership()
            .mutates_world()
            .then(
                CommandNode::argument("from", ArgumentType::container()).then(
                    CommandNode::argument("to", ArgumentType::container()).executes(|ctx| {
                        let from = ctx.args().get_container("from")?;
                        let to = ctx.args().get_container("to")?;
                        let (first_pos, second_pos) = ctx.get_selection()?;

                        let start_time = Instant::now();

                        let new_block = match to {
                            ContainerType::Furnace => Block::Furnace {},
                            ContainerType::Barrel => Block::Barrel {},
                            ContainerType::Hopper => Block::Hopper {},
                        };
                        let slots = to.num_slots() as u32;

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
                                        if *ty != from {
                                            continue;
                                        }
                                        let ss = *comparator_override;

                                        let items_needed = match ss {
                                            0 => 0,
                                            15 => slots * 64,
                                            _ => ((32 * slots * ss as u32) as f32 / 7.0 - 1.0)
                                                .ceil()
                                                as u32,
                                        }
                                            as usize;

                                        let mut inventory = Vec::new();
                                        for (slot, items_added) in
                                            (0..items_needed).step_by(64).enumerate()
                                        {
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
                    }),
                ),
            ),
    );
}
