use crate::commands::error::{CommandResult, RuntimeError};
use crate::commands::{
    argument::ArgumentType, context::ExecutionContext, node::CommandNode, registry::CommandRegistry,
};
use crate::plot::Plot;
use crate::server::Message;
use crate::worldedit::ray_trace_block;
use crate::{
    player::{Gamemode, PlayerPos},
    plot::database,
};
use mchprs_blocks::items::ItemStack;
use mchprs_network::PlayerPacketSender;
use mchprs_redpiler::CompilerOptions;
use mchprs_save_data::plot_data::{Tps, WorldSendRate};
use std::time::Instant;
use tracing::debug;

pub(super) fn register_commands(registry: &mut CommandRegistry) {
    registry.register(CommandNode::literal("speed").then(
        CommandNode::argument("speed", ArgumentType::float(0.0, 10.0)).executes(|ctx| {
            let speed = ctx.args().get_float("speed")?;

            let player = ctx.player_mut()?;
            player.fly_speed = speed;
            player.update_player_abilities();
            let username = player.username.clone();
            ctx.reply(&format!("Set flying speed to {} for {}", speed, username))?;
            Ok(())
        }),
    ));

    registry.register(
        CommandNode::literal("teleport")
            .alias("tp")
            .require_permission("mchprs.teleport")
            .then(
                CommandNode::argument("position", ArgumentType::vec3()).executes(|ctx| {
                    let position = ctx.args().get_vec3("position")?;

                    let player_pos = ctx.player()?.pos;
                    let (x, y, z) = position.resolve((player_pos.x, player_pos.y, player_pos.z));

                    let player = ctx.player_mut()?;
                    player.teleport(PlayerPos::new(x, y, z));
                    ctx.reply(&format!("Teleporting to ({}, {}, {})", x, y, z))?;
                    Ok(())
                }),
            )
            .then(
                CommandNode::argument("player", ArgumentType::player()).executes(|ctx| {
                    let username = ctx.args().get_player("player")?;
                    ctx.reply(&format!("Teleporting to {}", username))?;

                    let player_idx = ctx.player_index()?;
                    let uuid = ctx.plot.players[player_idx].uuid;
                    let player = ctx.plot.leave_plot(uuid);
                    ctx.plot
                        .send_message(Message::PlayerTeleportOther(player, username));
                    Ok(())
                }),
            ),
    );

    fn exec_rtps_display(ctx: &mut ExecutionContext<'_>) -> CommandResult<()> {
        let report = ctx.plot.generate_timings_report();
        let tps = ctx.plot.tps();
        let message = if let Some(report) = report {
            format!(
                "&6RTPS from last 10s, 1m, 5m, 15m: &a{:.1}, {:.1}, {:.1}, {:.1} ({})",
                report.ten_s, report.one_m, report.five_m, report.fifteen_m, tps
            )
        } else {
            format!("&6No timings data. &a({})", tps)
        };
        let player = ctx.player()?;
        player.send_chat_message(&mchprs_text::TextComponent::from_legacy_text(&message));
        Ok(())
    }

    fn exec_rtps_set(ctx: &mut ExecutionContext<'_>, tps: Tps) -> CommandResult<()> {
        ctx.plot.set_tps(tps);
        ctx.reply(&format!("The rtps was successfully set to {}.", tps))
    }

    registry.register(
        CommandNode::literal("rtps")
            .executes(exec_rtps_display)
            .then(
                CommandNode::literal("unlimited")
                    .alias("u")
                    .require_permission("mchprs.rtps")
                    .executes(|ctx| exec_rtps_set(ctx, Tps::Unlimited)),
            )
            .then(
                CommandNode::argument("tps", ArgumentType::integer(0, 10_000_000))
                    .require_permission("mchprs.rtps")
                    .executes(|ctx| {
                        let tps = ctx.args().get_integer("tps")?;
                        exec_rtps_set(ctx, Tps::Limited(tps as u32))
                    }),
            ),
    );

    registry.register(
        CommandNode::literal("stop")
            .require_permission("mchprs.stop")
            .executes(|ctx| {
                ctx.plot.send_message(Message::Shutdown);
                ctx.reply("Stopping server...")
            }),
    );

    registry.register(
        CommandNode::literal("gamemode")
            .require_permission("mchprs.gamemode")
            .then(CommandNode::literal("creative").alias("1").executes(|ctx| {
                let player_idx = ctx.player_index()?;
                ctx.plot
                    .change_player_gamemode(player_idx, Gamemode::Creative);
                Ok(())
            }))
            .then(
                CommandNode::literal("spectator")
                    .alias("3")
                    .executes(|ctx| {
                        let player_idx = ctx.player_index()?;
                        ctx.plot
                            .change_player_gamemode(player_idx, Gamemode::Spectator);
                        Ok(())
                    }),
            ),
    );
    registry.add_custom_alias("gmc", "gamemode creative");
    registry.add_custom_alias("gmsp", "gamemode spectator");

    registry.register(
        CommandNode::literal("radvance")
            .alias("radv")
            .require_permission("mchprs.radvance")
            .then(
                CommandNode::argument("ticks", ArgumentType::integer(1, 1000000)).executes(|ctx| {
                    let ticks = ctx.args().get_integer("ticks")?;

                    let start_time = Instant::now();
                    ctx.plot.tickn(ticks as u64);

                    if ctx.plot.redpiler.is_active() {
                        ctx.plot.redpiler.flush(&mut ctx.plot.world);
                    }

                    ctx.reply(&format!(
                        "Plot has been advanced by {} ticks ({:?})",
                        ticks,
                        start_time.elapsed()
                    ))
                }),
            ),
    );

    registry.register(
        CommandNode::literal("toggleautorp")
            .require_permission("mchprs.toggleautorp")
            .executes(|ctx| {
                ctx.plot.auto_redpiler = !ctx.plot.auto_redpiler;
                if ctx.plot.auto_redpiler {
                    ctx.reply("Automatic redpiler compilation has been enabled.")
                } else {
                    ctx.reply("Automatic redpiler compilation has been disabled.")
                }
            }),
    );

    registry.register(
        CommandNode::literal("worldsendrate")
            .alias("wsr")
            .require_permission("mchprs.worldsendrate")
            .then(
                CommandNode::argument("hertz", ArgumentType::integer(1, 1000)).executes(|ctx| {
                    let hertz = ctx.args().get_integer("hertz")?;

                    ctx.plot.set_world_send_rate(WorldSendRate(hertz as u32));
                    ctx.reply("The world send rate was successfully set.")
                }),
            ),
    );

    registry.register(
        CommandNode::literal("container")
            .require_permission("mchprs.container")
            .then(
                CommandNode::argument("type", ArgumentType::container()).then(
                    CommandNode::argument("power", ArgumentType::integer(1, 15)).executes(|ctx| {
                        let container_type = ctx.args().get_container("type")?;
                        let power = ctx.args().get_integer("power")?;

                        let item = ItemStack::container_with_ss(container_type, power as u8);
                        let player = ctx.player_mut()?;
                        let slot = 36 + player.selected_slot;
                        player.set_inventory_slot(slot, Some(item));
                        ctx.reply("Container created.")
                    }),
                ),
            ),
    );

    registry.register(
        CommandNode::literal("whitelist")
            .require_permission("mchprs.whitelist")
            .then(CommandNode::literal("add").then(
                CommandNode::argument("username", ArgumentType::string()).executes(|ctx| {
                    let username = ctx.args().get_string("username")?;
                    let player = ctx.player()?;
                    let packet_sender = PlayerPacketSender::new(&player.client);
                    ctx.plot.whitelist_add(username.clone(), packet_sender);
                    ctx.reply(&format!("Adding {} to whitelist...", username))
                }),
            ))
            .then(CommandNode::literal("remove").then(
                CommandNode::argument("username", ArgumentType::string()).executes(|ctx| {
                    let username = ctx.args().get_string("username")?;
                    let player = ctx.player()?;
                    let packet_sender = PlayerPacketSender::new(&player.client);
                    ctx.plot.whitelist_remove(username.clone(), packet_sender);
                    ctx.reply(&format!("Removing {} from whitelist...", username))
                }),
            )),
    );

    fn exec_plot_visit(ctx: &mut ExecutionContext<'_>, index: Option<i32>) -> CommandResult<()> {
        let username = ctx.args().get_string("username")?;

        let plots = database::get_owned_plots(&username);
        if plots.is_empty() {
            return ctx.reply(&format!("{} does not own any plots.", username));
        }

        let (plot_x, plot_z) = if let Some(index) = index {
            let idx = (index - 1).max(0) as usize;
            if let Some(&pos) = plots.get(idx) {
                pos
            } else {
                return ctx.reply(&format!("Plot range (1, {}).", plots.len()));
            }
        } else {
            plots[0]
        };

        let center = Plot::get_center(plot_x, plot_z);
        ctx.player_mut()?
            .teleport(PlayerPos::new(center.0, 64.0, center.1));

        if let Some(index) = index {
            ctx.reply(&format!("Teleported to {}'s plot #{}.", username, index))
        } else {
            ctx.reply(&format!("Teleported to {}'s plot.", username))
        }
    }

    registry.register(
        CommandNode::literal("plot")
            .alias("p")
            .then(
                CommandNode::literal("info")
                    .alias("i")
                    .require_permission("plots.info")
                    .executes(|ctx| {
                        let (plot_x, plot_z) = ctx.player()?.pos.plot_pos();
                        if let Some(owner) = database::get_plot_owner(plot_x, plot_z) {
                            let username =
                                database::get_cached_username(owner.clone()).unwrap_or(owner);
                            ctx.reply(&format!("Plot owner is: {}", username))
                        } else {
                            ctx.reply("Plot is not owned by anyone.")
                        }
                    }),
            )
            .then(
                CommandNode::literal("claim")
                    .alias("c")
                    .require_permission("plots.claim")
                    .executes(|ctx| {
                        let (plot_x, plot_z) = ctx.player()?.pos.plot_pos();
                        if database::is_claimed(plot_x, plot_z).unwrap() {
                            ctx.reply("Plot is already claimed!")
                        } else {
                            let player_idx = ctx.player_index()?;
                            ctx.plot.claim_plot(plot_x, plot_z, player_idx);
                            Ok(())
                        }
                    }),
            )
            .then(
                CommandNode::literal("auto")
                    .alias("a")
                    .require_permission("plots.auto")
                    .executes(|ctx| {
                        let mut start = (0, 0);
                        for _ in 0..i32::MAX {
                            if database::is_claimed(start.0, start.1).unwrap() {
                                start = Plot::get_next_plot(start.0, start.1);
                            } else {
                                let player_idx = ctx.player_index()?;
                                ctx.plot.claim_plot(start.0, start.1, player_idx);
                                break;
                            }
                        }
                        Ok(())
                    }),
            )
            .then(
                CommandNode::literal("middle")
                    .require_permission("plots.middle")
                    .executes(|ctx| {
                        let (plot_x, plot_z) = ctx.player()?.pos.plot_pos();
                        let center = Plot::get_center(plot_x, plot_z);
                        ctx.player_mut()?
                            .teleport(PlayerPos::new(center.0, 64.0, center.1));
                        ctx.reply("Teleported to plot middle.")
                    }),
            )
            .then(
                CommandNode::literal("visit")
                    .alias("v")
                    .require_permission("plots.visit")
                    .then(
                        CommandNode::argument("username", ArgumentType::string())
                            .executes(|ctx| exec_plot_visit(ctx, None))
                            .then(
                                CommandNode::argument("index", ArgumentType::integer(1, 999))
                                    .executes(|ctx| {
                                        exec_plot_visit(ctx, Some(ctx.args().get_integer("index")?))
                                    }),
                            ),
                    ),
            )
            .then(
                CommandNode::literal("teleport")
                    .alias("tp")
                    .require_permission("plots.visit")
                    .then(
                        CommandNode::argument("location", ArgumentType::column_pos()).executes(
                            |ctx| {
                                let pos = ctx.args().get_column_pos("location")?;
                                let current_plot_pos = ctx.player()?.pos.plot_pos();
                                let (new_plot_x, new_plot_z) = pos.resolve(current_plot_pos);

                                let center = Plot::get_center(new_plot_x, new_plot_z);
                                ctx.player_mut()?
                                    .teleport(PlayerPos::new(center.0, 64.0, center.1));
                                ctx.reply(&format!(
                                    "Teleported to plot ({}, {}).",
                                    new_plot_x, new_plot_z
                                ))
                            },
                        ),
                    ),
            )
            .then(
                CommandNode::literal("lock")
                    .require_permission("plots.lock")
                    .executes(|ctx| {
                        let entity_id = ctx.player()?.entity_id;
                        if ctx.plot.add_locked_player(entity_id) {
                            let world = ctx.world();
                            let (x, z) = (world.x, world.z);
                            ctx.reply(&format!(
                                "Locked to plot ({}, {}). Use '/p unlock' to unlock.",
                                x, z
                            ))
                        } else {
                            ctx.reply("You are already locked to this plot.")
                        }
                    }),
            )
            .then(
                CommandNode::literal("unlock")
                    .require_permission("plots.lock")
                    .executes(|ctx| {
                        let entity_id = ctx.player()?.entity_id;
                        if ctx.plot.remove_locked_player(entity_id) {
                            ctx.reply("You are now unlocked.")
                        } else {
                            ctx.reply("You are not locked to this plot.")
                        }
                    }),
            )
            .then(
                CommandNode::literal("select")
                    .alias("sel")
                    .require_permission("plots.select")
                    .executes(|ctx| {
                        let corners = ctx.world().get_corners();
                        let player = ctx.player_mut()?;
                        player.worldedit_set_first_pos(corners.0);
                        player.worldedit_set_second_pos(corners.1);
                        Ok(())
                    }),
            ),
    );

    fn exec_redpiler_compile(ctx: &mut ExecutionContext<'_>) -> CommandResult<()> {
        let flags = ctx.args().get_flags("options")?;

        let options = CompilerOptions {
            optimize: flags.contains("optimize"),
            export: flags.contains("export"),
            io_only: flags.contains("io-only"),
            update: flags.contains("update"),
            export_dot_graph: flags.contains("export-dot"),
            wire_dot_out: flags.contains("wire-dot-out"),
            print_after_all: flags.contains("print-after-all"),
            print_before_backend: flags.contains("print-before-backend"),
            ..Default::default()
        };

        if options.optimize {
            let msg =
                "Redpiler optimization is highly unstable and can break builds. Use with caution!";
            tracing::warn!("{}", msg);
            ctx.reply(msg)?;
        }

        ctx.plot.reset_redpiler();
        let start_time = Instant::now();
        ctx.plot.start_redpiler(options);
        let duration = start_time.elapsed();
        let msg = format!("Compilation completed in {:?}", duration);
        debug!(msg);
        ctx.reply(&msg)?;
        Ok(())
    }

    let redpiler_flag_arg = ArgumentType::flags()
        .add('o', "optimize", "Enable redpiler optimizations")
        .add('e', "export", "Export the compile graph")
        .add(
            'i',
            "io-only",
            "Only send block updates of relevant input/output blocks",
        )
        .add('u', "update", "Update all blocks after redpiler resets")
        .add(
            'd',
            "wire-dot-out",
            "Consider wires in dot shape as output block",
        )
        .add(
            None,
            "export-dot",
            "Create a graphviz dot file of backend graph",
        )
        .add(None, "print-after-all", "Print after all passes")
        .add(None, "print-before-backend", "Print before backend");

    registry.register(
        CommandNode::literal("redpiler")
            .alias("rp")
            .then(
                CommandNode::literal("compile")
                    .alias("c")
                    .require_permission("mchprs.redpiler.compile")
                    .executes(exec_redpiler_compile)
                    .then(
                        CommandNode::argument("options", redpiler_flag_arg)
                            .executes(exec_redpiler_compile),
                    ),
            )
            .then(
                CommandNode::literal("inspect")
                    .alias("i")
                    .require_permission("mchprs.redpiler.inspect")
                    .executes(|ctx| {
                        let player = ctx.player()?;
                        let pitch = player.pitch as f64;
                        let yaw = player.yaw as f64;
                        let player_pos = player.pos;
                        let pos = match ray_trace_block(ctx.world(), player_pos, pitch, yaw, 10.0) {
                            Some(pos) => pos,
                            None => return Err(RuntimeError::NoBlockInSight.into()),
                        };

                        ctx.plot.redpiler.inspect(pos);
                        Ok(())
                    }),
            )
            .then(
                CommandNode::literal("reset")
                    .alias("r")
                    .require_permission("mchprs.redpiler.reset")
                    .executes(|ctx| {
                        ctx.plot.reset_redpiler();
                        ctx.reply("Redpiler has been reset.")
                    }),
            ),
    );
}
