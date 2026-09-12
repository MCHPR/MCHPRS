use crate::{
    commands::{
        argument::ArgumentType,
        context::ExecutionContext,
        define::{alt, arg, Command},
        error::{CommandResult, RuntimeError},
        registry::CommandRegistry,
        value::{PlayerTarget, PlotCoordinates},
    },
    player::{Gamemode, PlayerPos},
    plot::{database, Plot},
    server::{get_version_string, Message},
    utils::HyphenatedUUID,
};
use mchprs_blocks::block_entities::ContainerType;
use mchprs_blocks::items::ItemStack;
use mchprs_network::PlayerPacketSender;
use mchprs_redpiler::CompilerOptions;
use mchprs_save_data::plot_data::{Tps, WorldSendRate};
use std::time::Instant;
use tracing::{debug, warn};

pub(super) fn register_commands(registry: &mut CommandRegistry) {
    registry.register(
        Command::new("speed")
            .arg("speed", ArgumentType::float(0.0, 10.0))
            .executes(|ctx| {
                let speed: f32 = ctx.arg("speed")?;
                let player = ctx.player_mut()?;
                player.fly_speed = speed;
                player.update_player_abilities();
                let username = player.username.clone();
                ctx.reply(&format!("Set flying speed to {} for {}", speed, username))
            }),
    );

    registry.register(
        Command::new("teleport")
            .alias("tp")
            .syntax(alt([
                arg("position", ArgumentType::Vec3),
                arg("player", ArgumentType::PlayerTarget),
            ]))
            .executes(|ctx| {
                if let Some(position) = ctx.position("position")? {
                    let PlayerPos { x, y, z } = position;
                    if x.abs() > 30_000_000.0 || z.abs() > 30_000_000.0 || y.abs() > 20_000_000.0 {
                        return Err(RuntimeError::CoordinateOutOfRange.into());
                    }
                    ctx.player_mut()?.teleport(position);
                    return ctx.reply(&format!("Teleporting to ({}, {}, {})", x, y, z));
                }

                let target: PlayerTarget = ctx.arg("player")?;
                let username = match &target {
                    PlayerTarget::SelfPlayer => return ctx.reply("Teleported to yourself."),
                    PlayerTarget::Name(name) => name.clone(),
                    PlayerTarget::Uuid(uuid) => HyphenatedUUID(*uuid).to_string(),
                };
                ctx.reply(&format!("Teleporting to {}", username))?;
                let uuid = ctx.player()?.uuid;
                let player = ctx.plot_mut().leave_plot(uuid);
                ctx.plot_mut()
                    .send_message(Message::PlayerTeleportOther(player, target));
                Ok(())
            }),
    );

    fn exec_rtps_set(ctx: &mut ExecutionContext<'_>, tps: Tps) -> CommandResult<()> {
        ctx.plot_mut().set_tps(tps);
        ctx.reply(&format!("The rtps was successfully set to {}.", tps))
    }

    registry.register(
        Command::new("rtps")
            .subcommand(
                Command::new("unlimited")
                    .alias("u")
                    .executes(|ctx| exec_rtps_set(ctx, Tps::Unlimited)),
            )
            .optional("tps", ArgumentType::integer(0, 1_000_000_000))
            .executes(|ctx| {
                if let Some(tps) = ctx.arg_opt::<i32>("tps")? {
                    return exec_rtps_set(ctx, Tps::Limited(tps as u32));
                }

                let report = ctx.plot_mut().generate_timings_report();
                let tps = ctx.plot_mut().tps();
                let message = match report {
                    Some(report) => format!(
                        "&6RTPS from last 10s, 1m, 5m, 15m: &a{:.1}, {:.1}, {:.1}, {:.1} ({})",
                        report.ten_s, report.one_m, report.five_m, report.fifteen_m, tps
                    ),
                    None => format!("&6No timings data. &a({})", tps),
                };
                ctx.reply_legacy(&message)
            }),
    );

    registry.register(Command::new("stop").executes(|ctx| {
        ctx.plot_mut().send_message(Message::Shutdown);
        ctx.reply("Stopping server...")
    }));

    fn set_gamemode(ctx: &mut ExecutionContext<'_>, gamemode: Gamemode) -> CommandResult<()> {
        let player_idx = ctx.player_index();
        ctx.plot_mut().change_player_gamemode(player_idx, gamemode);
        Ok(())
    }

    registry.register(
        Command::new("gamemode")
            .subcommand(
                Command::new("creative")
                    .alias("1")
                    .executes(|ctx| set_gamemode(ctx, Gamemode::Creative)),
            )
            .subcommand(
                Command::new("spectator")
                    .alias("3")
                    .executes(|ctx| set_gamemode(ctx, Gamemode::Spectator)),
            ),
    );
    registry.add_custom_alias("gmc", "gamemode creative");
    registry.add_custom_alias("gmsp", "gamemode spectator");

    registry.register(
        Command::new("radvance")
            .alias("radv")
            .arg("ticks", ArgumentType::integer(1, 1_000_000_000))
            .executes(|ctx| {
                let ticks: i32 = ctx.arg("ticks")?;

                let start_time = Instant::now();
                let plot = ctx.plot_mut();
                plot.tickn(ticks as u64);

                if plot.redpiler.is_active() {
                    plot.redpiler.flush(&mut plot.world);
                }

                ctx.reply(&format!(
                    "Plot has been advanced by {} ticks ({:?})",
                    ticks,
                    start_time.elapsed()
                ))
            }),
    );

    registry.register(Command::new("toggleautorp").executes(|ctx| {
        let plot = ctx.plot_mut();
        plot.auto_redpiler = !plot.auto_redpiler;
        if plot.auto_redpiler {
            ctx.reply("Automatic redpiler compilation has been enabled.")
        } else {
            ctx.reply("Automatic redpiler compilation has been disabled.")
        }
    }));

    registry.register(
        Command::new("worldsendrate")
            .alias("wsr")
            .optional("hertz", ArgumentType::integer(1, 1000))
            .executes(|ctx| {
                if let Some(hertz) = ctx.arg_opt::<i32>("hertz")? {
                    ctx.plot_mut()
                        .set_world_send_rate(WorldSendRate(hertz as u32));
                    return ctx.reply("The world send rate was successfully set.");
                }
                let rate = ctx.plot_mut().world_send_rate();
                ctx.reply(&format!("Current world send rate: {} Hz", rate.0))
            }),
    );

    registry.register(Command::new("version").executes(|ctx| ctx.reply(&get_version_string())));

    registry.register(
        Command::new("container")
            .arg("type", ArgumentType::ContainerType)
            .arg("power", ArgumentType::integer(0, 15))
            .executes(|ctx| {
                let container_type: ContainerType = ctx.arg("type")?;
                let power: i32 = ctx.arg("power")?;

                let item = ItemStack::container_with_ss(container_type, power as u8);
                let player = ctx.player_mut()?;
                let slot = 36 + player.selected_slot;
                player.set_inventory_slot(slot, Some(item));
                ctx.reply("Container created.")
            }),
    );

    registry.register(
        Command::new("whitelist")
            .subcommand(
                Command::new("add")
                    .arg("username", ArgumentType::Word)
                    .executes(|ctx| {
                        let username: String = ctx.arg("username")?;
                        let packet_sender = PlayerPacketSender::new(&ctx.player()?.client);
                        ctx.reply(&format!("Adding {} to whitelist...", username))?;
                        ctx.plot_mut().whitelist_add(username, packet_sender);
                        Ok(())
                    }),
            )
            .subcommand(
                Command::new("remove")
                    .arg("username", ArgumentType::Word)
                    .executes(|ctx| {
                        let username: String = ctx.arg("username")?;
                        let packet_sender = PlayerPacketSender::new(&ctx.player()?.client);
                        ctx.plot_mut().whitelist_remove(username, packet_sender);
                        Ok(())
                    }),
            ),
    );

    fn teleport_to_plot(
        ctx: &mut ExecutionContext<'_>,
        plot_x: i32,
        plot_z: i32,
    ) -> CommandResult<()> {
        let center = Plot::get_center(plot_x, plot_z);
        if center.0.abs() > 30_000_000.0 || center.1.abs() > 30_000_000.0 {
            return Err(RuntimeError::CoordinateOutOfRange.into());
        }
        ctx.player_mut()?
            .teleport(PlayerPos::new(center.0, 64.0, center.1));
        Ok(())
    }

    registry.register(
        Command::new("plot")
            .alias("p")
            .subcommand(
                Command::new("info")
                    .alias("i")
                    .permission("plots.info")
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
            .subcommand(
                Command::new("claim")
                    .alias("c")
                    .permission("plots.claim")
                    .executes(|ctx| {
                        let (plot_x, plot_z) = ctx.player()?.pos.plot_pos();
                        if database::is_claimed(plot_x, plot_z).unwrap() {
                            ctx.reply("Plot is already claimed!")
                        } else {
                            let player_idx = ctx.player_index();
                            ctx.plot_mut().claim_plot(plot_x, plot_z, player_idx);
                            Ok(())
                        }
                    }),
            )
            .subcommand(
                Command::new("auto")
                    .alias("a")
                    .permission("plots.auto")
                    .executes(|ctx| {
                        let mut plot = (0, 0);
                        while database::is_claimed(plot.0, plot.1).unwrap() {
                            plot = Plot::get_next_plot(plot.0, plot.1);
                        }
                        let player_idx = ctx.player_index();
                        ctx.plot_mut().claim_plot(plot.0, plot.1, player_idx);
                        Ok(())
                    }),
            )
            .subcommand(
                Command::new("middle")
                    .permission("plots.middle")
                    .executes(|ctx| {
                        let (plot_x, plot_z) = ctx.player()?.pos.plot_pos();
                        teleport_to_plot(ctx, plot_x, plot_z)?;
                        ctx.reply("Teleported to plot middle.")
                    }),
            )
            .subcommand(
                Command::new("visit")
                    .alias("v")
                    .permission("plots.visit")
                    .arg("username", ArgumentType::Word)
                    .optional("index", ArgumentType::integer(1, 999))
                    .executes(|ctx| {
                        let username: String = ctx.arg("username")?;
                        let index = ctx.arg_opt::<i32>("index")?;

                        let plots = database::get_owned_plots(&username);
                        if plots.is_empty() {
                            return ctx.reply(&format!("{} does not own any plots.", username));
                        }

                        let Some(&(plot_x, plot_z)) = plots.get(index.unwrap_or(1) as usize - 1)
                        else {
                            return ctx.reply(&format!("Plot range (1, {}).", plots.len()));
                        };
                        teleport_to_plot(ctx, plot_x, plot_z)?;

                        match index {
                            Some(index) => {
                                ctx.reply(&format!("Teleported to {}'s plot #{}.", username, index))
                            }
                            None => ctx.reply(&format!("Teleported to {}'s plot.", username)),
                        }
                    }),
            )
            .subcommand(
                Command::new("teleport")
                    .alias("tp")
                    .permission("plots.visit")
                    .arg("location", ArgumentType::PlotPos)
                    .executes(|ctx| {
                        let location: PlotCoordinates = ctx.arg("location")?;
                        let current_plot_pos = ctx.player()?.pos.plot_pos();
                        let (plot_x, plot_z) = location.resolve(current_plot_pos)?;
                        teleport_to_plot(ctx, plot_x, plot_z)?;
                        ctx.reply(&format!("Teleported to plot ({}, {}).", plot_x, plot_z))
                    }),
            )
            .subcommand(
                Command::new("lock")
                    .permission("plots.lock")
                    .executes(|ctx| {
                        let entity_id = ctx.player()?.entity_id;
                        if ctx.plot_mut().add_locked_player(entity_id) {
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
            .subcommand(
                Command::new("unlock")
                    .permission("plots.lock")
                    .executes(|ctx| {
                        let entity_id = ctx.player()?.entity_id;
                        if ctx.plot_mut().remove_locked_player(entity_id) {
                            ctx.reply("You are now unlocked.")
                        } else {
                            ctx.reply("You are not locked to this plot.")
                        }
                    }),
            )
            .subcommand(
                Command::new("select")
                    .alias("sel")
                    .permission("plots.select")
                    .executes(|ctx| {
                        let corners = ctx.world().get_corners();
                        let player = ctx.player_mut()?;
                        player.worldedit_set_first_pos(corners.0);
                        player.worldedit_set_second_pos(corners.1);
                        Ok(())
                    }),
            ),
    );

    registry.register(
        Command::new("redpiler")
            .alias("rp")
            .subcommand(
                Command::new("compile")
                    .alias("c")
                    .flag('o', "optimize", "Enable redpiler optimizations")
                    .flag('e', "export", "Export the compile graph")
                    .flag(
                        'i',
                        "io-only",
                        "Only send block updates of relevant input/output blocks",
                    )
                    .flag('u', "update", "Update all blocks after redpiler resets")
                    .flag(
                        'd',
                        "wire-dot-out",
                        "Consider wires in dot shape as output block",
                    )
                    .flag(
                        'l',
                        "illegal-states-out",
                        "Consider wires in so-called \"illegal\" states as output block",
                    )
                    .flag(
                        'c',
                        "wire-cross-out",
                        "Consider a redstone cross to be an output block",
                    )
                    .flag(
                        None,
                        "export-dot",
                        "Create a graphviz dot file of backend graph",
                    )
                    .flag(None, "print-after-all", "Print after all passes")
                    .flag(None, "print-before-backend", "Print before backend")
                    .executes(exec_redpiler_compile),
            )
            .subcommand(Command::new("inspect").alias("i").executes(|ctx| {
                let pos = ctx.target_block(10.0)?;
                ctx.plot_mut().redpiler.inspect(pos);
                Ok(())
            }))
            .subcommand(Command::new("reset").alias("r").executes(|ctx| {
                ctx.plot_mut().reset_redpiler();
                ctx.reply("Redpiler has been reset.")
            })),
    );
}

fn exec_redpiler_compile(ctx: &mut ExecutionContext<'_>) -> CommandResult<()> {
    let options = CompilerOptions {
        optimize: ctx.flag("optimize"),
        export: ctx.flag("export"),
        io_only: ctx.flag("io-only"),
        update: ctx.flag("update"),
        export_dot_graph: ctx.flag("export-dot"),
        wire_dot_out: ctx.flag("wire-dot-out"),
        illegal_states_out: ctx.flag("illegal-states-out"),
        wire_cross_out: ctx.flag("wire-cross-out"),
        print_after_all: ctx.flag("print-after-all"),
        print_before_backend: ctx.flag("print-before-backend"),
        ..Default::default()
    };

    if options.optimize {
        let msg =
            "Redpiler optimization is highly unstable and can break builds. Use with caution!";
        warn!("{}", msg);
        ctx.reply(msg)?;
    }

    ctx.plot_mut().reset_redpiler();
    let start_time = Instant::now();
    ctx.plot_mut().start_redpiler(options);
    let duration = start_time.elapsed();
    let msg = format!("Compilation completed in {:?}", duration);
    debug!(msg);
    ctx.reply(&msg)
}
