use crate::network::{NetworkServer, NetworkState, NetworkClient};
use crate::network::packets::{
    PacketDecoder, 
    S00Handshake, 
    S00LoginStart, 
    C02LoginSuccess,
    C03SetCompression,
    ServerBoundPacket,
    ClientBoundPacket
};
use crate::permissions::Permissions;
use crate::player::Player;
use crossbeam::channel;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Messages get passed between plot threads, the server thread, and the networking thread.
/// These messages are used to communicated when a player joins, leaves, or moves into another plot,
/// as well as to communicate chat messages.
#[derive(Debug)]
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
            println!("{:#?}", message);
            match message {
                _ => {}
            }
        }
        self.network.update();
        for client in &mut self.network.handshaking_clients {
            let packets: Vec<PacketDecoder> = client.packets.drain(..).collect();
            for packet in packets {
                match client.state {
                    NetworkState::Handshake => {
                        if packet.packet_id == 0x00 {
                            let handshake = S00Handshake::decode(packet);
                            match handshake.next_state {
                                1 => client.state = NetworkState::Status,
                                2 => client.state = NetworkState::Login,
                                _ => {}
                            }
                        }
                    },
                    NetworkState::Status => {

                    },
                    NetworkState::Login => {
                        if packet.packet_id == 0x00 {
                            let login_start = S00LoginStart::decode(packet);
                            client.username = Some(login_start.name);
                            let set_compression = C03SetCompression {
                                threshold: 500
                            }.encode();
                            client.send_packet(set_compression);
                            client.compressed = true;
                            if let Some(username) = &client.username {
                                let login_success = C02LoginSuccess {
                                    uuid: Player::generate_offline_uuid(username),
                                    username: username.clone()
                                }.encode();
                                client.send_packet(login_success);
                                client.state = NetworkState::Play;
                            }
                            
                        }
                    },
                    NetworkState::Play => {}
                }
            }
            
        }
    }

}
