use crate::network::packets::*;
use crate::player::Player;
use crate::server::Message;
use bus::BusReader;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Cursor;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime};

#[derive(Debug, Serialize, Deserialize)]
struct PlotData {
    tps: i32,
    show_redstone: bool,
}

pub struct Plot {
    players: Vec<Player>,
    tps: u32,
    message_receiver: BusReader<Message>,
    message_sender: Sender<Message>,
    last_player_time: SystemTime,
    running: bool,
    x: i32,
    z: i32,
    show_redstone: bool,
    always_running: bool,
}

impl Plot {
    fn set_block(&mut self) {}

    fn enter_plot(&mut self, player: Player) {
        self.save();
        self.players.push(player);
    }

    fn tick(&mut self) {}

    fn update(&mut self) {
        // Handle messages from the message channel
        while let Ok(message) = self.message_receiver.try_recv() {
            // println!(
            //     "Plot({}, {}) received message: {:#?}",
            //     self.x, self.z, message
            // );
            match message {
                Message::PlayerTeleportOther(player, other_player) => {
                    for p in self.players.iter() {
                        if p.username == other_player {
                            let mut player = Arc::try_unwrap(player).unwrap();
                            player.teleport(p.x, p.y, p.z);
                            self.enter_plot(player);
                            break;
                        }
                    }
                }
                Message::PlayerEnterPlot(player, plot_x, plot_z) => {
                    if plot_x == self.x && plot_z == self.z {
                        let player = Arc::try_unwrap(player).unwrap();
                        self.enter_plot(player);
                    }
                }
                _ => {}
            }
        }
        // Only tick if there are players in the plot
        if !self.players.is_empty() {
            self.last_player_time = SystemTime::now();
            self.tick();
        } else {
            // Unload plot after 300 seconds unless the plot should be always loaded
            if self.last_player_time.elapsed().unwrap().as_secs() > 300 && !self.always_running {
                self.running = false;
            }
        }
        // Check if connected to player is still active
        for player in 0..self.players.len() {
            self.players[player].client.update();
            if !self.players[player].client.alive {
                let player = self.players.remove(player);
                player.save();
                self.message_sender.send(Message::PlayerLeft(player.uuid)).unwrap();
            }
        }
    }

    fn load(
        x: i32,
        z: i32,
        rx: BusReader<Message>,
        tx: Sender<Message>,
        always_running: bool,
    ) -> Plot {
        if let Ok(data) = fs::read(format!("./world/plots/p{}:{}", x, z)) {
            // TODO: Handle format error
            let plot_data: PlotData = nbt::from_reader(Cursor::new(data)).unwrap();
            Plot {
                last_player_time: SystemTime::now(),
                message_receiver: rx,
                message_sender: tx,
                players: Vec::new(),
                running: true,
                show_redstone: plot_data.show_redstone,
                tps: plot_data.tps as u32,
                x,
                z,
                always_running,
            }
        } else {
            Plot {
                last_player_time: SystemTime::now(),
                message_receiver: rx,
                message_sender: tx,
                players: Vec::new(),
                running: true,
                show_redstone: true,
                tps: 20,
                x,
                z,
                always_running,
            }
        }
    }

    fn save(&self) {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(format!("./world/plots/p{}:{}", self.x, self.z))
            .unwrap();
        nbt::to_writer(
            &mut file,
            &PlotData {
                tps: self.tps as i32,
                show_redstone: self.show_redstone,
            },
            None,
        )
        .unwrap();
        file.sync_data().unwrap();
    }

    fn run(&mut self) {
        println!("Running new plot!");
        while self.running {
            self.update();
            thread::sleep(Duration::from_millis(2));
        }
    }

    pub fn load_and_run(
        x: i32,
        z: i32,
        rx: BusReader<Message>,
        tx: Sender<Message>,
        always_running: bool,
    ) {
        let mut plot = Plot::load(x, z, rx, tx, always_running);
        thread::spawn(move || {
            plot.run();
        });
    }
}

impl Drop for Plot {
    fn drop(&mut self) {
        self.save();
        self.message_sender
            .send(Message::PlotUnload(self.x, self.z))
            .unwrap();
    }
}
