use super::Plot;
use crate::blocks::Block;
use crate::server::Message;

impl Plot {
    pub(super) fn handle_command(&mut self, player: usize, command: &str, args: Vec<&str>) {
        match command {
            "//1" | "//pos1" => {
                let player = &mut self.players[player];
                player.set_first_position(player.x as i32, player.y as i32, player.z as i32);
            }
            "//2" | "//pos2" => {
                let player = &mut self.players[player];
                player.set_second_position(player.x as i32, player.y as i32, player.z as i32);
            }
            "//set" => {
                let block = Block::from_name(&args[0]);
                if let Some(block) = block {
                    self.worldedit_set(player, block);
                } else {
                    self.players[player].send_system_message(
                        "Invalid block. Note that not all blocks are supported.",
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