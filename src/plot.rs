use crate::player::Player;
use crate::server::Message;
use crossbeam::channel;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Cursor;
use std::thread;
use std::time::{Duration, SystemTime};

#[derive(Debug, Serialize, Deserialize)]
struct PlotData {
    tps: i32,
    show_redstone: bool,
}

struct Plot {
    players: Vec<Player>,
    tps: u32,
    message_receiver: channel::Receiver<Message>,
    message_sender: channel::Sender<Message>,
    last_player_time: SystemTime,
    running: bool,
    x: u32,
    z: u32,
    show_redstone: bool,
}

impl Plot {
    fn set_block(&mut self) {}

    fn enter_plot(&mut self, player: Player) {
        self.players.push(player);
    }

    fn tick(&mut self) {}

    fn update(&mut self) {
        while let Ok(message) = self.message_receiver.try_recv() {
            match message {
                Message::PlayerTeleportOther(mut player, other_player) => {
                    for p in self.players.iter() {
                        if p.username == other_player {
                            player.teleport(p.x, p.y, p.z);
                            self.enter_plot(player);
                            break;
                        }
                    }
                }
                Message::PlayerEnterPlot(player, plot_x, plot_z) => {
                    if plot_x == self.x && plot_z == self.z {
                        self.enter_plot(player);
                    }
                }
                _ => {}
            }
        }
        if !self.players.is_empty() {
            self.tick();
        } else {
            // Unload plot after 300 seconds
            if self.last_player_time.elapsed().unwrap().as_secs() > 300 {
                self.running = false;
            }
        }
    }

    fn load(x: u32, z: u32, rx: channel::Receiver<Message>, tx: channel::Sender<Message>) -> Plot {
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
            }
        }
    }

    fn save(&self) {
        let mut file = File::open(format!("./world/plots/p{}:{}", self.x, self.z)).unwrap();
        nbt::to_writer(
            &mut file,
            &PlotData {
                tps: self.tps as i32,
                show_redstone: self.show_redstone,
            },
            None,
        )
        .unwrap();
    }

    fn run(&mut self) {
        while self.running {
            self.update();
            thread::sleep(Duration::from_millis(2));
        }
    }

    pub fn load_and_run(
        x: u32,
        y: u32,
        rx: channel::Receiver<Message>,
        tx: channel::Sender<Message>,
    ) {
        thread::spawn(move || {
            let mut plot = Plot::load(x, y, rx, tx);
            plot.run();
        });
    }
}

impl Drop for Plot {
    fn drop(&mut self) {
        self.message_sender
            .send(Message::PlotUnload(self.x, self.z))
            .unwrap();
    }
}