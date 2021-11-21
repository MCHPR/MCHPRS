use super::{database, worldedit, Plot};
use crate::blocks::ContainerType;
use crate::chat::ChatComponent;
use crate::items::ItemStack;
use crate::network::packets::clientbound::{
    CDeclareCommands, CDeclareCommandsNode as Node, CDeclareCommandsNodeParser as Parser,
    ClientBoundPacket,
};
use crate::network::packets::PacketEncoder;
use crate::player::{Gamemode, PlayerPos};
use crate::plot::PlotWorld;
use crate::profile::PlayerProfile;
use crate::redpiler::CompilerOptions;
use crate::server::Message;
use bitflags::_core::i32::MAX;
use log::{debug, info};
use std::lazy::SyncLazy;
use std::time::{Duration, Instant};

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
                for _ in 0..MAX {
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
                        Ok(idx) => idx - 1,
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

                let plot_x;
                let plot_z;
                if let Ok(x_arg) = args[0].parse() {
                    plot_x = x_arg;
                } else {
                    self.players[player].send_error_message("Unable to parse x coordinate!");
                    return;
                }
                if let Ok(z_arg) = args[1].parse() {
                    plot_z = z_arg;
                } else {
                    self.players[player].send_error_message("Unable to parse z coordinate!");
                    return;
                }

                let center = Plot::get_center(plot_x, plot_z);
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

                if options.use_worldedit {
                    if self.players[player].first_position.is_none() {
                        return;
                    }
                    if self.players[player].second_position.is_none() {
                        return;
                    }
                }

                let pos1 = self.players[player].first_position;
                let pos2 = self.players[player].second_position;

                self.reset_redpiler();
                self.start_redpiler(options, pos1, pos2);

                debug!("Compile took {:?}", start_time.elapsed());
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
        if worldedit::execute_command(self, player, &command[1..], &mut args) {
            // If the command was handled, there is no need to continue;
            return false;
        }

        match command {
            "/whitelist" => match args.as_slice() {
                ["add", username] => {
                    let username = username.to_string();
                    let sender = self.message_sender.clone();
                    self.async_rt.spawn(async move {
                        match PlayerProfile::lookup_by_username(&username).await {
                            Ok(profile) => sender
                                .send(Message::WhitelistAdd(profile.uuid.0, profile.username))
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
                    self.async_rt.spawn(async move {
                        match PlayerProfile::lookup_by_username(&username).await {
                            Ok(profile) => sender
                                .send(Message::WhitelistRemove(profile.uuid.0))
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
            "/rtps" => {
                if args.is_empty() {
                    let report = self.timings.generate_report();
                    if let Some(report) = report {
                        self.players[player].send_chat_message(
                            0,
                            &ChatComponent::from_legacy_text(&format!(
                                "&6RTPS from last 10s, 1m, 5m, 15m: &a{:.1}, {:.1}, {:.1}, {:.1} ({})",
                                report.ten_s, report.one_m, report.five_m, report.fifteen_m, self.tps
                            )),
                        );
                    } else {
                        self.players[player].send_chat_message(
                            0,
                            &ChatComponent::from_legacy_text(&format!(
                                "&6No timings data. &a({})",
                                self.tps
                            )),
                        );
                    }

                    return false;
                }
                let tps = if let Ok(tps) = args[0].parse::<u32>() {
                    tps
                } else {
                    self.players[player].send_error_message("Unable to parse rtps!");
                    return false;
                };
                if tps > 35000 {
                    self.players[player]
                        .send_error_message("The rtps cannot go higher than 35000!");
                    return false;
                }
                if tps > 10 {
                    self.sleep_time = Duration::from_micros(1_000_000 / tps as u64);
                } else {
                    self.sleep_time = Duration::from_millis(50);
                }
                self.timings.set_tps(tps);
                self.lag_time = Duration::from_millis(0);
                self.tps = tps;
                // This won't get set normally when rtps is 0
                self.last_update_time = Instant::now();
                self.players[player].send_system_message("The rtps was successfully set.");
            }
            "/radv" | "/radvance" => {
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
                for _ in 0..ticks {
                    self.tick();
                }
                self.players[player].send_system_message(&format!(
                    "Plot has been advanced by {} ticks ({:?})",
                    ticks,
                    start_time.elapsed()
                ));
            }
            "/teleport" | "/tp" => {
                if args.len() == 3 {
                    let x;
                    let y;
                    let z;
                    if let Ok(x_arg) = args[0].parse::<f64>() {
                        x = x_arg;
                    } else {
                        self.players[player].send_error_message("Unable to parse x coordinate!");
                        return false;
                    }
                    if let Ok(y_arg) = args[1].parse::<f64>() {
                        y = y_arg;
                    } else {
                        self.players[player].send_error_message("Unable to parse y coordinate!");
                        return false;
                    }
                    if let Ok(z_arg) = args[2].parse::<f64>() {
                        z = z_arg;
                    } else {
                        self.players[player].send_error_message("Unable to parse z coordinate!");
                        return false;
                    }
                    self.players[player].send_system_message(&format!("Teleporting to ({}, {}, {})", x, y, z));
                    self.players[player].teleport(PlayerPos::new(x, y, z));
                } else if args.len() == 1 {
                    self.players[player].send_system_message(&format!("Teleporting to {}", args[0]));
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
            "/stop" => {
                let _ = self.message_sender.send(Message::Shutdown);
            }
            "/plot" | "/p" => {
                if args.is_empty() {
                    self.players[player].send_error_message("Invalid number of arguments!");
                    return false;
                }
                let command = args.remove(0);
                self.handle_plot_command(player, command, &args);
            }
            "/redpiler" | "/rp" => {
                if args.is_empty() {
                    self.players[player].send_error_message("Invalid number of arguments!");
                    return false;
                }
                let command = args.remove(0);
                self.handle_redpiler_command(player, command, &args);
            }
            "/speed" => {
                if args.len() != 1 {
                    self.players[player].send_error_message("/speed <0-10>");
                    return false;
                }
                if let Ok(speed_arg) = args[0].parse::<f32>() {
                    if speed_arg < 0.0 {
                        self.players[player]
                            .send_error_message("Silly child, you can't have a negative flyspeed");
                        return false;
                    }
                    self.players[player].fly_speed = speed_arg;
                    self.players[player].update_player_abilities();
                    let username = self.players[player].username.clone();
                    self.players[player].send_system_message(&format!(
                        "Set flying speed to {} for {}",
                        speed_arg,
                        username
                    ));
                } else {
                    self.players[player].send_error_message("Unable to parse speed value");
                }
            }
            "/gmsp" => self.change_player_gamemode(player, Gamemode::Spectator),
            "/gmc" => self.change_player_gamemode(player, Gamemode::Creative),
            "/gamemode" => {
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
            "/container" => {
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

                let container_ty = match args[0] {
                    "barrel" => ContainerType::Barrel,
                    "furnace" => ContainerType::Furnace,
                    "hopper" => ContainerType::Hopper,
                    _ => {
                        self.players[player].send_error_message(
                            "Container type must be one of [barrel, furnace, hopper]",
                        );
                        return false;
                    }
                };

                let item = ItemStack::container_with_ss(container_ty, power);
                let slot = 36 + self.players[player].selected_slot;
                self.players[player].set_inventory_slot(slot, Some(item));
            }
            _ => self.players[player].send_error_message("Command not found!"),
        }
        false
    }
}

bitflags! {
    struct CommandFlags: u32 {
        const ROOT = 0x0;
        const LITERAL = 0x1;
        const ARGUMENT = 0x2;
        const EXECUTABLE = 0x4;
        const REDIRECT = 0x8;
    }
}

// In the future a DSL or some type of generation would be much better.
// For more information, see https://wiki.vg/Command_Data
/// The `DeclareCommands` packet that is sent when the player joins.
/// This is used for command autocomplete.
pub static DECLARE_COMMANDS: SyncLazy<PacketEncoder> = SyncLazy::new(|| {
    CDeclareCommands {
        nodes: &[
            // 0: Root Node
            Node {
                flags: CommandFlags::ROOT.bits() as i8,
                children: &[
                    1, 4, 5, 6, 11, 12, 14, 16, 18, 19, 20, 21, 22, 23, 24, 26, 29, 31, 32, 34, 36,
                    47, 49, 53, 60,
                ],
                redirect_node: None,
                name: None,
                parser: None,
            },
            // 1: /teleport
            Node {
                flags: CommandFlags::LITERAL.bits() as i8,
                children: &[2, 3],
                redirect_node: None,
                name: Some("teleport"),
                parser: None,
            },
            // 2: /teleport [x, y, z]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("x, y, z"),
                parser: Some(Parser::Vec3),
            },
            // 3: /teleport [player]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("player"),
                parser: Some(Parser::Entity(3)), // Only allow one player
            },
            // 4: /tp
            Node {
                flags: (CommandFlags::REDIRECT | CommandFlags::LITERAL).bits() as i8,
                children: &[],
                redirect_node: Some(1),
                name: Some("tp"),
                parser: None,
            },
            // 5: /stop
            Node {
                flags: (CommandFlags::EXECUTABLE | CommandFlags::LITERAL).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("stop"),
                parser: None,
            },
            // 6: /plot
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: &[7, 8, 9, 10, 38, 39, 40, 41, 43, 44, 46, 58, 59],
                redirect_node: None,
                name: Some("plot"),
                parser: None,
            },
            // 7: /plot info
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("info"),
                parser: None,
            },
            // 8: /plot i
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: &[],
                redirect_node: Some(7),
                name: Some("i"),
                parser: None,
            },
            // 9: /plot claim
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("claim"),
                parser: None,
            },
            // 10: /plot c
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: &[],
                redirect_node: Some(9),
                name: Some("c"),
                parser: None,
            },
            // 11: /p
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: &[],
                redirect_node: Some(6),
                name: Some("p"),
                parser: None,
            },
            // 12: /rtps
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[13],
                redirect_node: None,
                name: Some("rtps"),
                parser: None,
            },
            // 13: /rtps [rtps]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("rtps"),
                parser: Some(Parser::Integer(0, 35000)),
            },
            // 14: //pos1
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[15],
                redirect_node: None,
                name: Some("/pos1"),
                parser: None,
            },
            // 15: //pos1 [pos]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("pos"),
                parser: Some(Parser::BlockPos),
            },
            // 16: //pos2
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[17],
                redirect_node: None,
                name: Some("/pos2"),
                parser: None,
            },
            // 17: //pos2 [pos]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("pos"),
                parser: Some(Parser::BlockPos),
            },
            // 18: /1
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: &[],
                redirect_node: Some(14),
                name: Some("/1"),
                parser: None,
            },
            // 19: /2
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: &[],
                redirect_node: Some(16),
                name: Some("/2"),
                parser: None,
            },
            // 20: //copy
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[21],
                redirect_node: None,
                name: Some("/copy"),
                parser: None,
            },
            // 21: //c
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: &[],
                redirect_node: Some(20),
                name: Some("/c"),
                parser: None,
            },
            // 22: //paste
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[23],
                redirect_node: None,
                name: Some("/paste"),
                parser: None,
            },
            // 23: //p
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: &[],
                redirect_node: Some(20),
                name: Some("/p"),
                parser: None,
            },
            // 24: //set
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: &[25],
                redirect_node: None,
                name: Some("/set"),
                parser: None,
            },
            // 25: //set [block]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("block"),
                parser: Some(Parser::BlockState),
            },
            // 26: //replace
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: &[27],
                redirect_node: None,
                name: Some("/replace"),
                parser: None,
            },
            // 27: //replace [oldblock]
            Node {
                flags: (CommandFlags::ARGUMENT).bits() as i8,
                children: &[28],
                redirect_node: None,
                name: Some("oldblock"),
                parser: Some(Parser::BlockState),
            },
            // 28: //replace [oldblock] [newblock]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("newblock"),
                parser: Some(Parser::BlockState),
            },
            // 29: /radvance
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: &[30],
                redirect_node: None,
                name: Some("radvance"),
                parser: None,
            },
            // 30: /radvance [rticks]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("rticks"),
                parser: Some(Parser::Integer(0, 35000)),
            },
            // 31: /radv
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: &[],
                redirect_node: Some(29),
                name: Some("radv"),
                parser: None,
            },
            // 32: /speed
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: &[33],
                redirect_node: None,
                name: Some("speed"),
                parser: None,
            },
            // 33: /speed [speed]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("speed"),
                parser: Some(Parser::Float(0.0, 10.0)),
            },
            // 34: //stack
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[35],
                redirect_node: None,
                name: Some("/stack"),
                parser: None,
            },
            // 35: //stack [amount]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("amount"),
                parser: Some(Parser::Integer(0, 256)),
            },
            // 36: //undo
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("/undo"),
                parser: None,
            },
            // 37: //sel
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("/sel"),
                parser: None,
            },
            // 38: /p auto
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("auto"),
                parser: None,
            },
            // 39: /p a
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: &[],
                redirect_node: Some(9),
                name: Some("a"),
                parser: None,
            },
            // 40: /p middle
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("middle"),
                parser: None,
            },
            // 41: /p visit
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: &[42],
                redirect_node: None,
                name: Some("visit"),
                parser: None,
            },
            // 42: /p visit [player]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("player"),
                parser: Some(Parser::Entity(3)),
            },
            // 43: /p v
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: &[],
                redirect_node: Some(41),
                name: Some("v"),
                parser: None,
            },
            // 44: /p teleport
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: &[45],
                redirect_node: None,
                name: Some("teleport"),
                parser: None,
            },
            // 45: /p teleport [x, z]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("x, z"),
                parser: Some(Parser::Vec2),
            },
            // 46: /p tp
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: &[],
                redirect_node: Some(44),
                name: Some("tp"),
                parser: None,
            },
            // 47: //shift
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[35],
                redirect_node: None,
                name: Some("/shift"),
                parser: None,
            },
            // 48: //shift [amount]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("amount"),
                parser: Some(Parser::Integer(0, 256)),
            },
            // 49: /whitelist
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: &[50, 51],
                redirect_node: None,
                name: Some("whitelist"),
                parser: None,
            },
            // 50: /whitelist add
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: &[52],
                redirect_node: None,
                name: Some("add"),
                parser: None,
            },
            // 51: /whitelist remove
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: &[52],
                redirect_node: None,
                name: Some("remove"),
                parser: None,
            },
            // 52: /whitelist add|remove [username]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("username"),
                parser: Some(Parser::Entity(3)),
            },
            // 53: /container
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: &[54, 55, 56],
                redirect_node: None,
                name: Some("container"),
                parser: None,
            },
            // 54: /container barrel
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: &[57],
                redirect_node: None,
                name: Some("barrel"),
                parser: None,
            },
            // 55: /container hopper
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: &[57],
                redirect_node: None,
                name: Some("hopper"),
                parser: None,
            },
            // 56: /container furnace
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: &[57],
                redirect_node: None,
                name: Some("furnace"),
                parser: None,
            },
            // 57: /container [type] [power]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("power"),
                parser: Some(Parser::Integer(0, 15)),
            },
            // 58: /plot lock
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("lock"),
                parser: None,
            },
            // 59: /plot unlock
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("unlock"),
                parser: None,
            },
            // 60: //wand
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: &[],
                redirect_node: None,
                name: Some("/wand"),
                parser: None,
            },
        ],
        root_index: 0,
    }
    .encode()
});
