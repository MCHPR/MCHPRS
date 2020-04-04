use crate::network::packets::clientbound::{ClientBoundPacket, C02LoginSuccess, C03SetCompression, C26JoinGame, C19PluginMessageBrand};
use crate::network::packets::serverbound::{ServerBoundPacket, S00Handshake, S00LoginStart};
use crate::network::packets::PacketDecoder;
use crate::network::{NetworkServer, NetworkClient, NetworkState};
use crate::permissions::Permissions;
use crate::player::Player;
use crate::plot::Plot;
use bus::Bus;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Messages get passed between plot threads, the server thread, and the networking thread.
/// These messages are used to communicated when a player joins, leaves, or moves into another plot,
/// as well as to communicate chat messages.
#[derive(Debug, Clone)]
pub enum Message {
    Chat(String),
    PlayerJoined(Arc<Player>, PlayerInfo),
    PlayerLeft(u128),
    PlayerEnterPlot(Arc<Player>, i32, i32),
    PlayerTeleportOther(Arc<Player>, String),
    PlotUnload(i32, i32),
}

#[derive(Debug, Clone)]
struct PlayerInfo {
    plot_x: i32,
    plot_z: i32,
    username: String,
    uuid: u128,
}

struct PlotInfo {
    plot_x: i32,
    plot_z: i32,
}

/// This represents a minecraft server
pub struct MinecraftServer {
    network: NetworkServer,
    config: config::Config,
    broadcaster: Bus<Message>,
    receiver: Receiver<Message>,
    plot_sender: Sender<Message>,
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
        let permissions = Arc::new(Mutex::new(Permissions::new(&config)));
        let (plot_tx, server_rx) = mpsc::channel();
        let bus = Bus::new(100);
        let mut server = MinecraftServer {
            network: NetworkServer::new(bind_addr),
            config,
            broadcaster: bus,
            receiver: server_rx,
            plot_sender: plot_tx,
            permissions,
            online_players: Vec::new(),
            running_plots: Vec::new(),
        };
        // Load the spawn area plot on server start
        // This plot should be always active
        Plot::load_and_run(
            0,
            0,
            server.broadcaster.add_rx(),
            server.plot_sender.clone(),
            true,
        );
        server.running_plots.push(PlotInfo {
            plot_x: 0,
            plot_z: 0,
        });
        loop {
            server.update();
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn update(&mut self) {
        while let Ok(message) = self.receiver.try_recv() {
            println!("Main thread received message: {:#?}", message);
            match message {
                Message::PlayerJoined(player, player_info) => {
                    let plot_x = player_info.plot_x;
                    let plot_z = player_info.plot_z;
                    let plot_loaded = self
                        .running_plots
                        .iter()
                        .any(|p| p.plot_x == plot_x && p.plot_z == plot_z);
                    if !plot_loaded {
                        println!("Plot is not running, loading it now...");
                        Plot::load_and_run(
                            plot_x,
                            plot_z,
                            self.broadcaster.add_rx(),
                            self.plot_sender.clone(),
                            false,
                        );
                        self.running_plots.push(PlotInfo { plot_x, plot_z });
                    }
                    println!("Sending Player into Plot");
                    self.broadcaster
                        .broadcast(Message::PlayerEnterPlot(player, plot_x, plot_z));
                    self.online_players.push(player_info);
                }
                Message::PlayerLeft(uuid) => {
                    let index = self.online_players.iter().position(|p| p.uuid == uuid);
                    if let Some(index) = index {
                        self.online_players.remove(index);
                    }
                }
                Message::PlotUnload(plot_x, plot_z) => {
                    let index = self
                        .running_plots
                        .iter()
                        .position(|p| p.plot_x == plot_x && p.plot_z == plot_z);
                    if let Some(index) = index {
                        self.running_plots.remove(index);
                    }
                }
                _ => {}
            }
        }
        self.network.update();
        let clients = &mut self.network.handshaking_clients;
        for client in 0..clients.len() {
            let packets: Vec<PacketDecoder> = clients[client].packets.drain(..).collect();
            for packet in packets {
                match clients[client].state {
                    NetworkState::Handshake => {
                        if packet.packet_id == 0x00 {
                            let handshake = S00Handshake::decode(packet);
                            let client = &mut clients[client];
                            match handshake.next_state {
                                1 => client.state = NetworkState::Status,
                                2 => client.state = NetworkState::Login,
                                _ => {}
                            }
                        }
                    }
                    NetworkState::Status => {}
                    NetworkState::Login => {
                        if packet.packet_id == 0x00 {
                            let login_start = S00LoginStart::decode(packet);
                            clients[client].username = Some(login_start.name);
                            let set_compression = C03SetCompression { threshold: 500 }.encode();
                            clients[client].send_packet(set_compression);
                            clients[client].compressed = true;
                            let username = if let Some(name) = &clients[client].username {
                                name.clone()
                            } else {
                                Default::default()
                            };
                            let uuid = Player::generate_offline_uuid(&username);
                            let login_success = C02LoginSuccess {
                                uuid,
                                username: username.clone(),
                            }
                            .encode();
                            clients[client].send_packet(login_success);
                            clients[client].state = NetworkState::Play;
                            let mut client = clients.remove(client);
                            let join_game = C26JoinGame {
                                entity_id: client.id as i32,
                                gamemode: 1,
                                dimention: 0,
                                hash_seed: 0,
                                max_players: u8::MAX,
                                level_type: "default".to_string(),
                                view_distance: 8,
                                reduced_debug_info: false,
                                enable_respawn_screen: false
                            }.encode();
                            client.send_packet(join_game);
                            let brand = C19PluginMessageBrand {
                                brand: "Minecraft High Performace Redstone Server".to_string()
                            }.encode();
                            client.send_packet(brand);
                            let player = Player::load_player(uuid, username.clone(), client);
                            let plot_x = (player.x / 128f64).floor() as i32;
                            let plot_z = (player.y / 128f64).floor() as i32;
                            self.plot_sender
                                .send(Message::PlayerJoined(
                                    Arc::new(player),
                                    PlayerInfo {
                                        plot_x,
                                        plot_z,
                                        username,
                                        uuid,
                                    },
                                ))
                                .unwrap();
                        }
                    }
                    NetworkState::Play => {}
                }
            }
        }
    }
}
