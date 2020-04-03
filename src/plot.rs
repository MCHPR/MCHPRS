use crate::player::Player;
use crate::server::Message;
use crossbeam::channel;
use std::thread;
use std::time::{Duration, SystemTime};

struct Plot {
    players: Vec<Player>,
    tps: u32,
    message_receiver: channel::Receiver<Message>,
    message_sender: channel::Sender<Message>,
    last_player_time: SystemTime,
    running: bool,
    x: u32,
    z: u32,
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
                self.message_sender
                    .send(Message::PlotUnload(self.x, self.z));
            }
        }
    }

    fn load(x: u32, y: u32) -> Plot {}

    fn run(&mut self) {
        while self.running {
            self.update();
            thread::sleep(Duration::from_millis(2));
        }
    }

    pub fn load_and_run(x: u32, y: u32) {
        thread::spawn(|| {
            let plot = Plot::load(x, y);
            plot.run();
        });
    }
}
