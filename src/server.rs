use crate::network::NetworkServer;
use crate::permissions::Permissions;
use crate::player::Player;
use crossbeam::channel;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Messages get passed between plot threads, the server thread, and the networking thread.
/// These messages are used to communicated when a player joins, leaves, or moves into another plot,
/// as well as to communicate chat messages.
pub enum Message {
    Chat(String),
    PlayerJoined(Player),
    PlayerLeft(u32),
    PlayerEnterPlot(Player, u32, u32),
    PlayerTeleportOther(Player, String),
    PlotUnload(u32, u32),
}

struct PlayerInfo {
    plot_x: u32,
    plot_y: u32,
    username: String,
    uuid: u128,
}

struct PlotInfo {
    plot_x: u32,
    plot_y: u32,
}

/// This represents a minecraft server
pub struct MinecraftServer {
    network: NetworkServer,
    config: config::Config,
    message_sender: channel::Sender<Message>,
    message_receiver: channel::Receiver<Message>,
    permissions: Arc<Mutex<Permissions>>,
    online_players: Vec<PlayerInfo>,
    running_plots: Vec<PlotInfo>,
}

impl MinecraftServer {
    pub fn run() {
        println!("Starting server...");
        let mut config = config::Config::default();
        config
            .merge(config::File::with_name("Config"))
            .expect("Error reading config file!");
        let bind_addr = config
            .get_str("bind_address")
            .expect("Bind address not found in config file!");
        let (tx, rx) = channel::unbounded();
        let permissions = Arc::new(Mutex::new(Permissions::new(&config)));
        let mut server = MinecraftServer {
            network: NetworkServer::new(bind_addr),
            config,
            message_sender: tx,
            message_receiver: rx,
            permissions,
            online_players: Vec::new(),
            running_plots: Vec::new(),
        };
        loop {
            server.update();
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn update(&mut self) {
        while let Ok(message) = self.message_receiver.try_recv() {
            match message {
                _ => {}
            }
        }
        self.network.update();
    }
}
