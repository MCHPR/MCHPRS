use super::{database, worldedit, Plot, PlotWorld};
use crate::player::{Gamemode, PacketSender, PlayerPos};
use crate::plot::data::sleep_time_for_tps;
use crate::profile::PlayerProfile;
use crate::server::Message;
use mchprs_blocks::items::ItemStack;
use mchprs_network::packets::clientbound::{
    CCommands, CCommandsNode as Node, CDeclareCommandsNodeParser as Parser, ClientBoundPacket,
};
use mchprs_network::packets::PacketEncoder;
use mchprs_network::PlayerPacketSender;
use mchprs_redpiler::CompilerOptions;
use mchprs_save_data::plot_data::{Tps, WorldSendRate};
use mchprs_text::TextComponent;
use once_cell::sync::Lazy;
use std::ops::Add;
use std::str::FromStr;
use std::time::Instant;
use tracing::{debug, info, warn};

// Parses a relative or absolute coordinate relative to a reference coordinate
fn parse_relative_coord<F: FromStr + Add + Add<Output = F>>(
    coord: &str,
    ref_coord: F,
) -> Result<F, <F as FromStr>::Err> {
    if coord == "~" {
        Ok(ref_coord)
    } else if let Some(offset_str) = coord.strip_prefix('~') {
        offset_str.parse::<F>().map(|x| ref_coord + x)
    } else {
        coord.parse::<F>()
    }
}

impl Plot {
    /// Handles a command that starts with `/plot` or `/p`
    fn handle_plot_command(&mut self, player: usize, command: &str, args: &[&str]) {
        let (plot_x, plot_z) = self.players[player].pos.plot_pos();

        let permission_node = match command {
            "info" | "i" => "plots.info",
            "claim" | "c" => "plots.claim",
            "auto" | "a" => "plots.auto",
            "middle" => "plots.middle",
            "visit" | "v" => "plots.visit",
            "teleport" | "tp" => "plots.visit",
            "lock" | "unlock" => "plots.lock",
            "sel" | "select" => "plots.select",
            _ => {
                self.players[player].send_error_message("Invalid argument for /plot");
                return;
            }
        };
        if !self.players[player].has_permission(permission_node) {
            self.players[player].send_no_permission_message();
            return;
        }

        match command {
            "info" | "i" => {
                if let Some(owner) = database::get_plot_owner(plot_x, plot_z) {
                    self.players[player].send_system_message(&format!(
                        "Plot owner is: {}",
                        database::get_cached_username(owner.clone()).unwrap_or(owner)
                    ));
                } else {
                    self.players[player].send_system_message("Plot is not owned by anyone.");
                }
            }
            "claim" | "c" => {
                if database::is_claimed(plot_x, plot_z).unwrap() {
                    self.players[player].send_system_message("Plot is already claimed!");
                } else {
                    self.claim_plot(plot_x, plot_z, player);
                }
            }
            "auto" | "a" => {
                let mut start = (0, 0);
                for _ in 0..i32::MAX {
                    if database::is_claimed(start.0, start.1).unwrap() {
                        start = Plot::get_next_plot(start.0, start.1);
                    } else {
                        self.claim_plot(start.0, start.1, player);
                        break;
                    }
                }
            }
            "middle" => {
                let center = Plot::get_center(plot_x, plot_z);
                self.players[player].teleport(PlayerPos::new(center.0, 64.0, center.1));
            }
            "visit" | "v" => {
                if !(1..=2).contains(&args.len()) {
                    self.players[player].send_error_message("Invalid number of arguments!");
                    return;
                }

                let idx = if args.len() == 2 {
                    match args[1].parse::<usize>() {
                        Ok(idx) => idx.wrapping_sub(1),
                        Err(_) => {
                            self.players[player].send_error_message("Unable to parse index");
                            return;
                        }
                    }
                } else {
                    0
                };

                let plots = database::get_owned_plots(args[0]);
                if !plots.is_empty() {
                    if let Some(&(plot_x, plot_z)) = plots.get(idx) {
                        let center = Plot::get_center(plot_x, plot_z);
                        self.players[player].teleport(PlayerPos::new(center.0, 64.0, center.1));
                    } else {
                        self.players[player]
                            .send_system_message(&format!("Plot range (1, {}).", plots.len()));
                    }
                } else {
                    self.players[player]
                        .send_system_message(&format!("{} does not own any plots.", args[0]));
                }
            }
            "teleport" | "tp" => {
                if args.len() != 2 {
                    self.players[player].send_error_message("Invalid number of arguments!");
                    return;
                }

                let new_plot_x;
                let new_plot_z;
                if let Ok(x_arg) = parse_relative_coord(args[0], plot_x) {
                    new_plot_x = x_arg;
                } else {
                    self.players[player].send_error_message("Unable to parse x coordinate!");
                    return;
                }
                if let Ok(z_arg) = parse_relative_coord(args[1], plot_z) {
                    new_plot_z = z_arg;
                } else {
                    self.players[player].send_error_message("Unable to parse z coordinate!");
                    return;
                }

                let center = Plot::get_center(new_plot_x, new_plot_z);
                self.players[player].teleport(PlayerPos::new(center.0, 64.0, center.1));
            }
            "lock" => {
                if self.locked_players.insert(self.players[player].entity_id) {
                    let PlotWorld { x, z, .. } = self.world;
                    let res = format!("Locked to plot ({}, {}). Use '/p unlock' to unlock.", x, z);
                    self.players[player].send_system_message(&res);
                } else {
                    self.players[player]
                        .send_system_message("You are already locked to this plot.");
                }
            }
            "unlock" => {
                if self.locked_players.remove(&self.players[player].entity_id) {
                    self.players[player].send_system_message("You are now unlocked.");
                } else {
                    self.players[player].send_system_message("You are not locked to this plot.");
                }
            }
            "select" | "sel" => {
                let corners = self.world.get_corners();
                self.players[player].worldedit_set_first_position(corners.0);
                self.players[player].worldedit_set_second_position(corners.1);
            }
            _ => self.players[player].send_error_message("Invalid argument for /plot"),
        }
    }

    /// Handles a command that starts with `/redpiler` or `/rp`
    fn handle_redpiler_command(&mut self, player: usize, command: &str, args: &[&str]) {
        match command {
            "compile" | "c" => {
                let start_time = Instant::now();
                let args = args.join(" ");
                let options = CompilerOptions::parse(&args);

                if options.optimize {
                    let msg = "Redpiler optimization is highly unstable and can break builds. Use with caution!";
                    warn!("{}", msg);
                    self.players[player].send_system_message(msg);
                }

                self.reset_redpiler();
                self.start_redpiler(options);

                debug!("Compile took {:?}", start_time.elapsed());
            }
            "inspect" | "i" => {
                let player = &self.players[player];
                let pos = worldedit::ray_trace_block(
                    &self.world,
                    player.pos,
                    player.pitch as f64,
                    player.yaw as f64,
                    10.0,
                );
                let Some(pos) = pos else {
                    player.send_error_message("Trace failed");
                    return;
                };
                self.redpiler.inspect(pos);
            }
            "reset" | "r" => {
                self.reset_redpiler();
            }
            _ => self.players[player].send_error_message("Invalid argument for /redpiler"),
        }
    }

    // Returns true if packets should stop being handled
    pub(super) fn handle_command(
        &mut self,
        player: usize,
        command: &str,
        mut args: Vec<&str>,
    ) -> bool {
        info!(
            "{} issued command: {} {}",
            self.players[player].username,
            command,
            args.join(" ")
        );

        // Handle worldedit commands
        if worldedit::execute_command(self, player, command, &mut args) {
            // If the command was handled, there is no need to continue;
            return false;
        }

        match command {
            "whitelist" => match args.as_slice() {
                ["add", username] => {
                    let username = username.to_string();
                    let sender = self.message_sender.clone();
                    let packet_sender = PlayerPacketSender::new(&self.players[player].client);
                    self.async_rt.spawn(async move {
                        match PlayerProfile::lookup_by_username(&username).await {
                            Ok(profile) => sender
                                .send(Message::WhitelistAdd(
                                    profile.uuid.0,
                                    profile.username,
                                    packet_sender,
                                ))
                                .unwrap(),
                            Err(_) => {
                                debug!("Failed to look up profile for username {:?}", username)
                            }
                        }
                    });
                }
                ["remove", username] => {
                    let username = username.to_string();
                    let sender = self.message_sender.clone();
                    let packet_sender = PlayerPacketSender::new(&self.players[player].client);
                    self.async_rt.spawn(async move {
                        match PlayerProfile::lookup_by_username(&username).await {
                            Ok(profile) => sender
                                .send(Message::WhitelistRemove(profile.uuid.0, packet_sender))
                                .unwrap(),
                            Err(_) => {
                                debug!("Failed to look up profile for username {:?}", username)
                            }
                        }
                    });
                }
                _ => {
                    self.players[player]
                        .send_error_message("Usage: /whitelist [add | remove] (username)");
                    return false;
                }
            },
            "rtps" => {
                if args.is_empty() {
                    let report = self.timings.generate_report();
                    if let Some(report) = report {
                        self.players[player].send_chat_message(&TextComponent::from_legacy_text(
                            &format!(
                            "&6RTPS from last 10s, 1m, 5m, 15m: &a{:.1}, {:.1}, {:.1}, {:.1} ({})",
                            report.ten_s, report.one_m, report.five_m, report.fifteen_m, self.tps
                        ),
                        ));
                    } else {
                        self.players[player].send_chat_message(&TextComponent::from_legacy_text(
                            &format!("&6No timings data. &a({})", self.tps),
                        ));
                    }

                    return false;
                }

                let tps = if let Ok(tps) = args[0].parse::<f32>() {
                    if tps < 0.0 {
                        self.players[player].send_error_message("RTPS must be cannot be negative!");
                        return false;
                    }
                    Tps::Limited(tps)
                } else if !args[0].is_empty() && "unlimited".starts_with(args[0]) {
                    Tps::Unlimited
                } else {
                    self.players[player].send_error_message("Unable to parse rtps!");
                    return false;
                };

                self.sleep_time = sleep_time_for_tps(tps);
                self.timings.set_tps(tps);
                self.tps = tps;
                self.reset_timings();
                self.players[player].send_system_message("The rtps was successfully set.");
            }
            "radv" | "radvance" => {
                if args.is_empty() {
                    self.players[player]
                        .send_error_message("Please specify a number of ticks to advance.");
                    return false;
                }
                let ticks = if let Ok(ticks) = args[0].parse::<u32>() {
                    ticks
                } else {
                    self.players[player].send_error_message("Unable to parse ticks!");
                    return false;
                };
                let start_time = Instant::now();
                self.tickn(ticks as u64);

                if self.redpiler.is_active() {
                    self.redpiler.flush(&mut self.world);
                }
                self.players[player].send_system_message(&format!(
                    "Plot has been advanced by {} ticks ({:?})",
                    ticks,
                    start_time.elapsed()
                ));
            }
            "toggleautorp" => {
                self.auto_redpiler = !self.auto_redpiler;
                if self.auto_redpiler {
                    self.players[player]
                        .send_system_message("Automatic redpiler compilation has been enabled.");
                } else {
                    self.players[player]
                        .send_system_message("Automatic redpiler compilation has been disabled.");
                }
            }
            "teleport" | "tp" => {
                if args.len() == 3 {
                    let player_pos = self.players[player].pos;
                    let x;
                    let y;
                    let z;
                    if let Ok(x_arg) = parse_relative_coord(args[0], player_pos.x) {
                        x = x_arg;
                    } else {
                        self.players[player].send_error_message("Unable to parse x coordinate!");
                        return false;
                    }
                    if let Ok(y_arg) = parse_relative_coord(args[1], player_pos.y) {
                        y = y_arg;
                    } else {
                        self.players[player].send_error_message("Unable to parse y coordinate!");
                        return false;
                    }
                    if let Ok(z_arg) = parse_relative_coord(args[2], player_pos.z) {
                        z = z_arg;
                    } else {
                        self.players[player].send_error_message("Unable to parse z coordinate!");
                        return false;
                    }
                    self.players[player]
                        .send_system_message(&format!("Teleporting to ({}, {}, {})", x, y, z));
                    self.players[player].teleport(PlayerPos::new(x, y, z));
                } else if args.len() == 1 {
                    self.players[player]
                        .send_system_message(&format!("Teleporting to {}", args[0]));
                    let uuid = self.players[player].uuid;
                    let player = self.leave_plot(uuid);
                    let _ = self
                        .message_sender
                        .send(Message::PlayerTeleportOther(player, args[0].to_string()));
                    return true;
                } else {
                    self.players[player]
                        .send_error_message("Invalid number of arguments for teleport command!");
                }
            }
            "stop" => {
                let _ = self.message_sender.send(Message::Shutdown);
            }
            "plot" | "p" => {
                if args.is_empty() {
                    self.players[player].send_error_message("Invalid number of arguments!");
                    return false;
                }
                let command = args.remove(0);
                self.handle_plot_command(player, command, &args);
            }
            "redpiler" | "rp" => {
                if args.is_empty() {
                    self.players[player].send_error_message("Invalid number of arguments!");
                    return false;
                }
                let command = args.remove(0);
                self.handle_redpiler_command(player, command, &args);
            }
            "speed" => {
                if args.len() != 1 {
                    self.players[player].send_error_message("/speed <0-10>");
                    return false;
                }
                if let Ok(speed_arg) = args[0].parse::<f32>() {
                    if speed_arg < 0.0 {
                        self.players[player]
                            .send_error_message("Silly child, you can't have a negative flyspeed!");
                        return false;
                    }
                    if speed_arg > 10.0 {
                        self.players[player].send_error_message(
                            "For performance reasons player speed cannot be higher than 10.",
                        );
                        return false;
                    }
                    if speed_arg.is_nan() {
                        self.players[player]
                            .send_error_message("You can't set your speed to NaN or -NaN.");
                        return false;
                    }
                    self.players[player].fly_speed = speed_arg;
                    self.players[player].update_player_abilities();
                    let username = self.players[player].username.clone();
                    self.players[player].send_system_message(&format!(
                        "Set flying speed to {} for {}",
                        speed_arg, username
                    ));
                } else {
                    self.players[player].send_error_message("Unable to parse speed value");
                }
            }
            "gmsp" => self.change_player_gamemode(player, Gamemode::Spectator),
            "gmc" => self.change_player_gamemode(player, Gamemode::Creative),
            "gamemode" => {
                if args.is_empty() {
                    self.players[player].send_error_message("Invalid number of arguments!");
                    return false;
                }
                let name = args.remove(0);
                let gamemode = match name {
                    "creative" | "1" => Gamemode::Creative,
                    "spectator" | "3" => Gamemode::Spectator,
                    _ => {
                        self.players[player].send_error_message("Unknown gamemode");
                        return false;
                    }
                };
                self.change_player_gamemode(player, gamemode);
            }
            "container" => {
                if args.len() != 2 {
                    self.players[player].send_error_message("Usage: /container [type] [power]");
                    return false;
                }

                let power = if let Ok(p) = args[1].parse() {
                    p
                } else {
                    self.players[player].send_error_message("Unable to parse power!");
                    return false;
                };

                let container_ty = match args[0].parse() {
                    Ok(ty) => ty,
                    Err(()) => {
                        self.players[player].send_error_message(
                            "Container type must be one of [barrel, furnace, hopper]",
                        );
                        return false;
                    }
                };

                if !(1..=15).contains(&power) {
                    self.players[player].send_error_message(
                        "Container power must be greater than 0 and lower than 15!",
                    );
                    return false;
                }

                let item = ItemStack::container_with_ss(container_ty, power);
                let slot = 36 + self.players[player].selected_slot;
                self.players[player].set_inventory_slot(slot, Some(item));
            }
            "worldsendrate" | "wsr" => {
                if args.is_empty() {
                    self.players[player].send_system_message(&format!(
                        "Current world send rate: {:.2} Hz",
                        self.world_send_rate.0
                    ));
                    return false;
                }

                if args.len() != 1 {
                    self.players[player].send_error_message("Usage: /worldsendrate [hertz]");
                    return false;
                }

                let Ok(hertz) = args[0].parse::<f32>() else {
                    self.players[player].send_error_message("Unable to parse send rate!");
                    return false;
                };
                if hertz < 0.0 {
                    self.players[player]
                        .send_error_message("The world send rate cannot be negative!");
                    return false;
                }
                if hertz > 1000.0 {
                    self.players[player]
                        .send_error_message("The world send rate cannot be higher than 1000!");
                    return false;
                }

                self.world_send_rate = WorldSendRate(hertz);
                self.reset_timings();
                self.players[player]
                    .send_system_message("The world send rate was successfully set.");
            }
            _ => self.players[player].send_error_message("Command not found!"),
        }
        false
    }
}

bitflags! {
    pub struct CommandFlags: u32 {
        const ROOT = 0x0;
        const LITERAL = 0x1;
        const ARGUMENT = 0x2;
        const EXECUTABLE = 0x4;
        const REDIRECT = 0x8;
        const HAS_SUGGESTIONS_TYPE = 0x10;
    }
}

// In the future a DSL or some type of generation would be much better.
// For more information, see https://wiki.vg/Command_Data
/// The `DeclareCommands` packet that is sent when the player joins.
/// This is used for command autocomplete.
pub static DECLARE_COMMANDS: Lazy<PacketEncoder> = Lazy::new(|| {
    let mut packet = CCommands {
        nodes: vec![
            // 0: Root Node
            Node {
                flags: CommandFlags::ROOT.bits() as i8,
                children: vec![
                    1, 4, 5, 6, 8, 10, 11, 13, 18, 30, 34, 41, 43, 44, 45, 49, 51,
                ],
                redirect_node: None,
                name: None,
                parser: None,
                suggestions_type: None,
            },
            // 1: /teleport
            Node {
                flags: CommandFlags::LITERAL.bits() as i8,
                children: vec![2, 3],
                redirect_node: None,
                name: Some("teleport"),
                parser: None,
                suggestions_type: None,
            },
            // 2: /teleport [x, y, z]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("x, y, z"),
                parser: Some(Parser::Vec3),
                suggestions_type: None,
            },
            // 3: /teleport [player]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("player"),
                parser: Some(Parser::Entity(3)), // Only allow one player
                suggestions_type: None,
            },
            // 4: /tp
            Node {
                flags: (CommandFlags::REDIRECT | CommandFlags::LITERAL).bits() as i8,
                children: vec![],
                redirect_node: Some(1),
                name: Some("tp"),
                parser: None,
                suggestions_type: None,
            },
            // 5: /stop
            Node {
                flags: (CommandFlags::EXECUTABLE | CommandFlags::LITERAL).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("stop"),
                parser: None,
                suggestions_type: None,
            },
            // 6: /rtps
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![7],
                redirect_node: None,
                name: Some("rtps"),
                parser: None,
                suggestions_type: None,
            },
            // 7: /rtps [rtps]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("rtps"),
                parser: Some(Parser::Float(0.0, f32::MAX)),
                suggestions_type: None,
            },
            // 8: /radvance
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![9],
                redirect_node: None,
                name: Some("radvance"),
                parser: None,
                suggestions_type: None,
            },
            // 9: /radvance [rticks]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("rticks"),
                parser: Some(Parser::Integer(0, i32::MAX)),
                suggestions_type: None,
            },
            // 10: /radv
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(9),
                name: Some("radv"),
                parser: None,
                suggestions_type: None,
            },
            // 11: /speed
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![12],
                redirect_node: None,
                name: Some("speed"),
                parser: None,
                suggestions_type: None,
            },
            // 12: /speed [speed]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("speed"),
                parser: Some(Parser::Float(0.0, 10.0)),
                suggestions_type: None,
            },
            // 13: /plot
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![14, 15, 16, 17, 19, 20, 21, 22, 24, 25, 27, 28, 29],
                redirect_node: None,
                name: Some("plot"),
                parser: None,
                suggestions_type: None,
            },
            // 14: /plot info
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("info"),
                parser: None,
                suggestions_type: None,
            },
            // 15: /plot i
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(14),
                name: Some("i"),
                parser: None,
                suggestions_type: None,
            },
            // 16: /plot claim
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("claim"),
                parser: None,
                suggestions_type: None,
            },
            // 17: /plot c
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(9),
                name: Some("c"),
                parser: None,
                suggestions_type: None,
            },
            // 18: /p
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(13),
                name: Some("p"),
                parser: None,
                suggestions_type: None,
            },
            // 19: /p auto
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("auto"),
                parser: None,
                suggestions_type: None,
            },
            // 20: /p a
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(19),
                name: Some("a"),
                parser: None,
                suggestions_type: None,
            },
            // 21: /p middle
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("middle"),
                parser: None,
                suggestions_type: None,
            },
            // 22: /p visit
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![23],
                redirect_node: None,
                name: Some("visit"),
                parser: None,
                suggestions_type: None,
            },
            // 23: /p visit [player]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("player"),
                parser: Some(Parser::Entity(3)),
                suggestions_type: None,
            },
            // 24: /p v
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(22),
                name: Some("v"),
                parser: None,
                suggestions_type: None,
            },
            // 25: /p teleport
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![26],
                redirect_node: None,
                name: Some("teleport"),
                parser: None,
                suggestions_type: None,
            },
            // 26: /p teleport [x, z]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("x, z"),
                parser: Some(Parser::Vec2),
                suggestions_type: None,
            },
            // 27: /p tp
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(25),
                name: Some("tp"),
                parser: None,
                suggestions_type: None,
            },
            // 28: /p select
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("select"),
                parser: None,
                suggestions_type: None,
            },
            // 29: /p sel
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(28),
                name: Some("sel"),
                parser: None,
                suggestions_type: None,
            },
            // 30: /whitelist
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![31, 32],
                redirect_node: None,
                name: Some("whitelist"),
                parser: None,
                suggestions_type: None,
            },
            // 31: /whitelist add
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![33],
                redirect_node: None,
                name: Some("add"),
                parser: None,
                suggestions_type: None,
            },
            // 32: /whitelist remove
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![33],
                redirect_node: None,
                name: Some("remove"),
                parser: None,
                suggestions_type: None,
            },
            // 33: /whitelist add|remove [username]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("username"),
                parser: Some(Parser::Entity(3)),
                suggestions_type: None,
            },
            // 34: /container
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![35, 36, 37],
                redirect_node: None,
                name: Some("container"),
                parser: None,
                suggestions_type: None,
            },
            // 35: /container barrel
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![38],
                redirect_node: None,
                name: Some("barrel"),
                parser: None,
                suggestions_type: None,
            },
            // 36: /container hopper
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![38],
                redirect_node: None,
                name: Some("hopper"),
                parser: None,
                suggestions_type: None,
            },
            // 37: /container furnace
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![38],
                redirect_node: None,
                name: Some("furnace"),
                parser: None,
                suggestions_type: None,
            },
            // 38: /container [type] [power]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("power"),
                parser: Some(Parser::Integer(0, 15)),
                suggestions_type: None,
            },
            // 39: /plot lock
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("lock"),
                parser: None,
                suggestions_type: None,
            },
            // 40: /plot unlock
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("unlock"),
                parser: None,
                suggestions_type: None,
            },
            // 41: //load
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![42],
                redirect_node: None,
                name: Some("/load"),
                parser: None,
                suggestions_type: None,
            },
            // 42: //load [filename]
            Node {
                flags: (CommandFlags::ARGUMENT
                    | CommandFlags::EXECUTABLE
                    | CommandFlags::HAS_SUGGESTIONS_TYPE)
                    .bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("filename"),
                parser: Some(Parser::String(0)),
                suggestions_type: Some("minecraft:ask_server"),
            },
            // 43: /toggleautorp
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("toggleautorp"),
                parser: None,
                suggestions_type: None,
            },
            // 44: /redpiler
            Node {
                flags: CommandFlags::LITERAL.bits() as i8,
                children: vec![46, 47, 48], // Children are compile, inspect, reset
                redirect_node: None,
                name: Some("redpiler"),
                parser: None,
                suggestions_type: None,
            },
            // 45: /rp
            Node {
                flags: (CommandFlags::REDIRECT | CommandFlags::LITERAL).bits() as i8,
                children: vec![],
                redirect_node: Some(44), // Redirect to /redpiler
                name: Some("rp"),
                parser: None,
                suggestions_type: None,
            },
            // 46: /redpiler compile
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("compile"),
                parser: None,
                suggestions_type: None,
            },
            // 47: /redpiler inspect
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("inspect"),
                parser: None,
                suggestions_type: None,
            },
            // 48: /redpiler reset
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("reset"),
                parser: None,
                suggestions_type: None,
            },
            // 49: /worldsendrate
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![50],
                redirect_node: None,
                name: Some("worldsendrate"),
                parser: None,
                suggestions_type: None,
            },
            // 50: /worldsendrate [hertz]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("hertz"),
                parser: Some(Parser::Float(0.0, 1000.0)),
                suggestions_type: None,
            },
            // 51: /wsr
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(49),
                name: Some("wsr"),
                parser: None,
                suggestions_type: None,
            },
        ],
        root_index: 0,
    };
    super::worldedit::add_command_completions(&mut packet.nodes);
    packet.encode()
});
