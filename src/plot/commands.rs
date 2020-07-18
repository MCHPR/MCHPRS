use super::{database, Plot};
use crate::network::packets::clientbound::{
    C12DeclareCommands, C12DeclareCommandsNode as Node, C12DeclareCommandsNodeParser as Parser,
    C32PlayerAbilities, ClientBoundPacket,
};
use crate::network::packets::PacketEncoder;
use crate::server::Message;
use log::info;

use std::time::{Duration, Instant};

impl Plot {
    /// Handles a command that starts with `/plot` or `/p`
    fn handle_plot_command(&mut self, player: usize, command: &str, args: Vec<&str>) {
        let plot_x = self.players[player].x as i32 >> 8;
        let plot_z = self.players[player].z as i32 >> 8;
        match command {
            "claim" | "c" => {
                if database::get_plot_owner(plot_x, plot_z).is_some() {
                    self.players[player].send_system_message("Plot is already claimed!");
                } else {
                    let uuid = format!("{}", self.players[player].uuid);
                    database::claim_plot(plot_x, plot_z, &uuid);
                    self.players[player]
                        .send_system_message(&format!("Claimed plot {},{}", plot_x, plot_z));
                }
            }
            "info" | "i" => {
                if let Some(owner) = database::get_plot_owner(plot_x, plot_z) {
                    self.players[player]
                        .send_system_message(&format!("Plot owner is: {:032x}", owner));
                } else {
                    self.players[player].send_system_message("Plot is not owned by anyone.");
                }
            }
            _ => self.players[player].send_error_message("Wrong argument for /plot"),
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
        match command {
            "//1" | "//pos1" => {
                let player = &mut self.players[player];

                let mut x = player.x as i32;
                let mut y = player.y as u32;
                let mut z = player.z as i32;

                if args.len() >= 3 {
                    if let Ok(x_arg) = args[0].parse::<i32>() {
                        x = x_arg;
                    } else {
                        player.send_error_message("Unable to parse x coordinate!");
                        return false;
                    }
                    if let Ok(y_arg) = args[1].parse::<u32>() {
                        y = y_arg;
                    } else {
                        player.send_error_message("Unable to parse y coordinate!");
                        return false;
                    }
                    if let Ok(z_arg) = args[2].parse::<i32>() {
                        z = z_arg;
                    } else {
                        player.send_error_message("Unable to parse z coordinate!");
                        return false;
                    }
                }

                player.worldedit_set_first_position(x, y, z);
            }
            "//2" | "//pos2" => {
                let player = &mut self.players[player];

                let mut x = player.x as i32;
                let mut y = player.y as u32;
                let mut z = player.z as i32;

                if args.len() >= 3 {
                    if let Ok(x_arg) = args[0].parse::<i32>() {
                        x = x_arg;
                    } else {
                        player.send_error_message("Unable to parse x coordinate!");
                        return false;
                    }
                    if let Ok(y_arg) = args[1].parse::<u32>() {
                        y = y_arg;
                    } else {
                        player.send_error_message("Unable to parse y coordinate!");
                        return false;
                    }
                    if let Ok(z_arg) = args[2].parse::<i32>() {
                        z = z_arg;
                    } else {
                        player.send_error_message("Unable to parse z coordinate!");
                        return false;
                    }
                }

                player.worldedit_set_second_position(x, y, z);
            }
            "//set" => {
                if args.is_empty() {
                    self.players[player].send_error_message("Wrong number of arguments!");
                    return false;
                }
                if self.worldedit_set(player, &args[0]).is_err() {
                    self.players[player].send_error_message(
                        "Invalid block. Note that not all blocks are supported.",
                    );
                }
            }
            "//replace" => {
                if args.len() < 2 {
                    self.players[player].send_error_message("Wrong number of arguments!");
                    return false;
                }
                if self.worldedit_replace(player, &args[0], &args[1]).is_err() {
                    self.players[player].send_error_message(
                        "Invalid block. Note that not all blocks are supported.",
                    );
                }
            }
            "//find" => {
                if args.is_empty() {
                    self.players[player].send_error_message("Wrong number of arguments!");
                    return false;
                }
                self.worldedit_find(player, args[0].parse::<u32>().unwrap())
            }
            "//copy" | "//c" => self.worldedit_copy(player),
            "//paste" | "//p" => self.worldedit_paste(player),
            "//count" => {
                if args.is_empty() {
                    self.players[player].send_error_message("Wrong number of arguments!");
                    return false;
                }
                if self.worldedit_count(player, &args[0]).is_err() {
                    self.players[player].send_error_message(
                        "Invalid block. Note that not all blocks are supported.",
                    );
                }
            }
            "//load" => {
                if args.is_empty() {
                    self.players[player].send_error_message("Wrong number of arguments!");
                    return false;
                }
                self.worldedit_load(player, &args[0])
            }
            "/rtps" => {
                if args.is_empty() {
                    self.players[player]
                        .send_error_message("Please specify the rtps you want to set to.");
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
                    self.sleep_time = Duration::from_millis(2);
                }
                self.lag_time = Duration::from_millis(0);
                self.tps = tps;
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
                    self.players[player].teleport(x, y, z);
                } else if args.len() == 1 {
                    let player = self.leave_plot(player);
                    self.message_sender
                        .send(Message::PlayerTeleportOther(player, args[0].to_string()));
                    return true;
                } else {
                    self.players[player]
                        .send_error_message("Wrong number of arguments for teleport command!");
                }
            }
            "/stop" => {
                self.message_sender.send(Message::Shutdown);
            }
            "/plot" | "/p" => {
                if args.is_empty() {
                    self.players[player].send_error_message("Wrong number of arguments!");
                    return false;
                }
                let command = args.remove(0);
                self.handle_plot_command(player, command, args);
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
                    } else if speed_arg > 10.0 {
                        self.players[player]
                            .send_error_message("You cannot have a flyspeed greater than 10");
                        return false;
                    }
                    let player_abilities = C32PlayerAbilities {
                        flags: 0x0F,
                        fly_speed: 0.05 * speed_arg,
                        fov_modifier: 0.1,
                    }
                    .encode();
                    self.players[player].client.send_packet(&player_abilities);
                } else {
                    self.players[player].send_error_message("Unable to parse speed value");
                }
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

lazy_static! {
    // In the future a DSL or some type of generation would be much better.
    // For more information, see https://wiki.vg/Command_Data
    /// The DeclareCommands packet that is sent when the player joins.
    /// This is used for command autocomplete.
    pub static ref DECLARE_COMMANDS: PacketEncoder = C12DeclareCommands {
        nodes: vec![
            // 0: Root Node
            Node {
                flags: CommandFlags::ROOT.bits() as i8,
                children: vec![1, 4, 5, 6, 11, 12, 14, 16, 18, 19, 20, 21, 22, 23, 24, 26, 29, 31, 32],
                redirect_node: None,
                name: None,
                parser: None,
            },
            // 1: /teleport
            Node {
                flags: CommandFlags::LITERAL.bits() as i8,
                children: vec![2, 3],
                redirect_node: None,
                name: Some("teleport"),
                parser: None,
            },
            // 2: /teleport [x, y, z]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("x, y, z"),
                parser: Some(Parser::Vec3),
            },
            // 3: /teleport [player]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("player"),
                parser: Some(Parser::Entity(3)), // Only allow one player
            },
            // 4: /tp
            Node {
                flags: (CommandFlags::REDIRECT | CommandFlags::LITERAL).bits() as i8,
                children: vec![],
                redirect_node: Some(1),
                name: Some("tp"),
                parser: None,
            },
            // 5: /stop
            Node {
                flags: (CommandFlags::EXECUTABLE | CommandFlags::LITERAL).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("stop"),
                parser: None,
            },
            // 6: /plot
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![7, 8, 9, 10],
                redirect_node: None,
                name: Some("plot"),
                parser: None,
            },
            // 7: /plot info
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("info"),
                parser: None,
            },
            // 8: /plot i
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(7),
                name: Some("i"),
                parser: None,
            },
            // 9: /plot claim
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("claim"),
                parser: None,
            },
            // 10: /plot c
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(9),
                name: Some("c"),
                parser: None,
            },
            // 11: /p
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(6),
                name: Some("p"),
                parser: None,
            },
            // 12: /rtps
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![13],
                redirect_node: None,
                name: Some("rtps"),
                parser: None,
            },
            // 13: /rtps [rtps]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("rtps"),
                parser: Some(Parser::Integer(0, 35000)),
            },
            // 14: //pos1
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![15],
                redirect_node: None,
                name: Some("/pos1"),
                parser: None,
            },
            // 15: //pos1 [pos]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("pos"),
                parser: Some(Parser::BlockPos),
            },
            // 16: //pos2
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![17],
                redirect_node: None,
                name: Some("/pos2"),
                parser: None,
            },
            // 17: //pos2 [pos]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("pos"),
                parser: Some(Parser::BlockPos),
            },
            // 18: /1
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(14),
                name: Some("/1"),
                parser: None,
            },
            // 19: /2
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(16),
                name: Some("/2"),
                parser: None,
            },
            // 20: //copy
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![21],
                redirect_node: None,
                name: Some("/copy"),
                parser: None,
            },
            // 21: //c
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(20),
                name: Some("/c"),
                parser: None,
            },
            // 22: //paste
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![23],
                redirect_node: None,
                name: Some("/paste"),
                parser: None,
            },
            // 23: //p
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(20),
                name: Some("/p"),
                parser: None,
            },
            // 24: //set
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![25],
                redirect_node: None,
                name: Some("/set"),
                parser: None,
            },
            // 25: //set [block]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("block"),
                parser: Some(Parser::BlockState),
            },
            // 26: //replace
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![27],
                redirect_node: None,
                name: Some("/replace"),
                parser: None,
            },
            // 27: //replace [oldblock]
            Node {
                flags: (CommandFlags::ARGUMENT).bits() as i8,
                children: vec![28],
                redirect_node: None,
                name: Some("oldblock"),
                parser: Some(Parser::BlockState),
            },
            // 28: //replace [oldblock] [newblock]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("newblock"),
                parser: Some(Parser::BlockState),
            },
            // 29: /radvance
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![30],
                redirect_node: None,
                name: Some("radvance"),
                parser: None,
            },
            // 30: /radvance [rticks]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("rticks"),
                parser: Some(Parser::Integer(0, 35000)),
            },
            // 31: /radv
            Node {
                flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
                children: vec![],
                redirect_node: Some(29),
                name: Some("radv"),
                parser: None,
            },
            // 32: /speed
            Node {
                flags: (CommandFlags::LITERAL).bits() as i8,
                children: vec![33],
                redirect_node: None,
                name: Some("speed"),
                parser: None,
            },
            // 33: /speed [speed]
            Node {
                flags: (CommandFlags::ARGUMENT | CommandFlags::EXECUTABLE).bits() as i8,
                children: vec![],
                redirect_node: None,
                name: Some("speed"),
                //A Parser::Float would be needed here (command still executes with floats though)
                parser: Some(Parser::Integer(0, 35000)),
            }
        ],
        root_index: 0
    }.encode();
}
