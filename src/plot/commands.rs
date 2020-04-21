use super::Plot;
use crate::blocks::Block;
use crate::plot::worldedit::WorldEditPattern;
use crate::server::Message;
use log::info;

impl Plot {
    pub(super) fn handle_command(&mut self, player: usize, command: &str, args: Vec<&str>) {
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
                let mut y = player.y as i32;
                let mut z = player.z as i32;

                if args.len() >= 3 {
                    if let Ok(x_arg) = args[0].parse::<i32>() {
                        x = x_arg;
                    } else {
                        player.send_system_message("Unable to parse x coordinate!");
                        return;
                    }
                    if let Ok(y_arg) = args[1].parse::<i32>() {
                        y = y_arg;
                    } else {
                        player.send_system_message("Unable to parse y coordinate!");
                        return;
                    }
                    if let Ok(z_arg) = args[2].parse::<i32>() {
                        z = z_arg;
                    } else {
                        player.send_system_message("Unable to parse z coordinate!");
                        return;
                    }
                }

                player.worldedit_set_first_position(x, y, z);
            }
            "//2" | "//pos2" => {
                let player = &mut self.players[player];

                let mut x = player.x as i32;
                let mut y = player.y as i32;
                let mut z = player.z as i32;

                if args.len() >= 3 {
                    if let Ok(x_arg) = args[0].parse::<i32>() {
                        x = x_arg;
                    } else {
                        player.send_system_message("Unable to parse x coordinate!");
                        return;
                    }
                    if let Ok(y_arg) = args[1].parse::<i32>() {
                        y = y_arg;
                    } else {
                        player.send_system_message("Unable to parse y coordinate!");
                        return;
                    }
                    if let Ok(z_arg) = args[2].parse::<i32>() {
                        z = z_arg;
                    } else {
                        player.send_system_message("Unable to parse z coordinate!");
                        return;
                    }
                }

                player.worldedit_set_second_position(x, y, z);
            }
            "//set" => {
                if let Err(_) = self.worldedit_set(player, &args[0]) {
                    self.players[player].send_system_message(
                        "Invalid block. Note that not all blocks are supported."
                    );
                }
            }
            "//replace" => {
                if let Err(_) = self.worldedit_replace(player, &args[0], &args[1]) {
                    self.players[player].send_system_message(
                        "Invalid block. Note that not all blocks are supported."
                    );
                }
            }
            "/tp" => {
                if args.len() == 3 {
                    let x;
                    let y;
                    let z;
                    if let Ok(x_arg) = args[0].parse::<f64>() {
                        x = x_arg;
                    } else {
                        self.players[player].send_system_message("Unable to parse x coordinate!");
                        return;
                    }
                    if let Ok(y_arg) = args[1].parse::<f64>() {
                        y = y_arg;
                    } else {
                        self.players[player].send_system_message("Unable to parse y coordinate!");
                        return;
                    }
                    if let Ok(z_arg) = args[2].parse::<f64>() {
                        z = z_arg;
                    } else {
                        self.players[player].send_system_message("Unable to parse z coordinate!");
                        return;
                    }
                    self.players[player].teleport(x, y, z);
                } else if args.len() == 1 {
                    self.players[player].send_system_message("TODO: teleport to player");
                } else {
                    self.players[player]
                        .send_system_message("Wrong number of arguments for teleport command!");
                }
            }
            "/stop" => {
                self.message_sender.send(Message::Shutdown);
            }
            _ => self.players[player].send_system_message("Command not found!"),
        }
    }
}
