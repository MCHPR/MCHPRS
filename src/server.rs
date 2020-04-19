use crate::network::packets::clientbound::{
    C00DisconnectLogin, C00Response, C01Pong, C02LoginSuccess, C03SetCompression, C15WindowItems,
    C19PluginMessageBrand, C26JoinGame, C34PlayerInfo, C34PlayerInfoAddPlayer,
    C36PlayerPositionAndLook, ClientBoundPacket,
};
use crate::network::packets::serverbound::{
    S00Handshake, S00LoginStart, S00Ping, ServerBoundPacket,
};
use crate::network::packets::{PacketDecoder, SlotData};
use crate::network::{NetworkServer, NetworkState};
//use crate::permissions::Permissions;
use crate::player::{Item, Player};
use crate::plot::Plot;
use bus::{Bus, BusReader};
use serde_json::json;
use std::fs;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Messages get passed between plot threads, the server thread, and the networking thread.
/// These messages are used to communicate when a player joins, leaves, or moves into another plot,
/// as well as to communicate chat messages.
#[derive(Debug, Clone)]
pub enum Message {
    Chat(String),
    PlayerJoinedInfo(PlayerJoinInfo),
    PlayerJoined(Arc<Player>),
    PlayerLeft(u128),
    PlayerLeavePlot(Arc<Player>),
    PlayerTeleportOther(Arc<Player>, String),
    PlotUnload(i32, i32),
}

#[derive(Debug)]
pub enum PrivMessage {
    PlayerEnterPlot(Player),
}

#[derive(Debug, Clone)]
pub struct PlayerJoinInfo {
    pub username: String,
    pub uuid: u128,
    skin: Option<String>,
}

#[derive(Debug, Clone)]
struct PlayerListEntry {
    plot_x: i32,
    plot_z: i32,
    username: String,
    uuid: u128,
    skin: Option<String>,
}

struct PlotListEntry {
    plot_x: i32,
    plot_z: i32,
    priv_message_sender: mpsc::Sender<PrivMessage>,
}

/// This represents a minecraft server
pub struct MinecraftServer {
    network: NetworkServer,
    config: config::Config,
    broadcaster: Bus<Message>,
    debug_plot_receiver: BusReader<Message>,
    receiver: Receiver<Message>,
    plot_sender: Sender<Message>,
    //permissions: Arc<Mutex<Permissions>>,
    online_players: Vec<PlayerListEntry>,
    running_plots: Vec<PlotListEntry>,
}

impl MinecraftServer {
    pub fn run() {
        println!("Starting server...");
        let start_time = Instant::now();
        fs::create_dir_all("./world/players").unwrap();
        fs::create_dir_all("./world/plots").unwrap();
        let mut config = config::Config::default();
        config
            .merge(config::File::with_name("Config"))
            .expect("Error reading config file!");
        let bind_addr = config
            .get_str("bind_address")
            .expect("Bind address not found in config file!");
        //let permissions = Arc::new(Mutex::new(Permissions::new(&config)));
        let (plot_tx, server_rx) = mpsc::channel();
        let mut bus = Bus::new(100);
        let debug_plot_receiver = bus.add_rx();
        let mut server = MinecraftServer {
            network: NetworkServer::new(bind_addr),
            config,
            broadcaster: bus,
            receiver: server_rx,
            plot_sender: plot_tx,
            debug_plot_receiver,
            // permissions,
            online_players: Vec::new(),
            running_plots: Vec::new(),
        };
        // Load the spawn area plot on server start
        // This plot should be always active
        let (spawn_tx, spawn_rx) = mpsc::channel();
        Plot::load_and_run(
            0,
            0,
            server.broadcaster.add_rx(),
            server.plot_sender.clone(),
            spawn_rx,
            true,
            None,
        );
        server.running_plots.push(PlotListEntry {
            plot_x: 0,
            plot_z: 0,
            priv_message_sender: spawn_tx,
        });
        println!("Done! Start took {:?}", start_time.elapsed());
        loop {
            server.update();
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn update_player_entry(&mut self, uuid: u128, plot_x: i32, plot_z: i32) {
        let player = self.online_players.iter_mut().find(|p| p.uuid == uuid);
        if let Some(player) = player {
            player.plot_x = plot_x;
            player.plot_z = plot_z;
        }
    }

    fn update(&mut self) {
        while let Ok(message) = self.debug_plot_receiver.try_recv() {
            println!("Main thread broadcasted message: {:#?}", message);
        }
        while let Ok(message) = self.receiver.try_recv() {
            println!("Main thread received message: {:#?}", message);
            match message {
                Message::PlayerJoined(player_arc) => {
                    let player = Arc::try_unwrap(player_arc).unwrap();
                    // Check if plot is loaded
                    let plot_x = (player.x as i32) >> 7;
                    let plot_z = (player.z as i32) >> 7;
                    let plot_loaded = self
                        .running_plots
                        .iter()
                        .any(|p| p.plot_x == plot_x && p.plot_z == plot_z);
                    // Add player to the player list
                    let player_list_entry = PlayerListEntry {
                        plot_x,
                        plot_z,
                        username: player.username.clone(),
                        uuid: player.uuid,
                        skin: None,
                    };
                    self.online_players.push(player_list_entry);
                    // Send player info to plots
                    let player_join_info = PlayerJoinInfo {
                        username: player.username.clone(),
                        uuid: player.uuid,
                        skin: None,
                    };
                    self.broadcaster
                        .broadcast(Message::PlayerJoinedInfo(player_join_info));
                    // Load the plot if it's not loaded
                    if !plot_loaded {
                        let (priv_tx, priv_rx) = mpsc::channel();
                        Plot::load_and_run(
                            plot_x,
                            plot_z,
                            self.broadcaster.add_rx(),
                            self.plot_sender.clone(),
                            priv_rx,
                            false,
                            Some(player),
                        );
                        self.running_plots.push(PlotListEntry {
                            plot_x,
                            plot_z,
                            priv_message_sender: priv_tx,
                        });
                    } else {
                        let plot_list_entry = self
                            .running_plots
                            .iter()
                            .find(|p| p.plot_x == plot_x && p.plot_z == plot_z)
                            .unwrap();
                        plot_list_entry
                            .priv_message_sender
                            .send(PrivMessage::PlayerEnterPlot(player));
                    }
                }
                Message::PlayerLeft(uuid) => {
                    let index = self.online_players.iter().position(|p| p.uuid == uuid);
                    if let Some(index) = index {
                        self.online_players.remove(index);
                    }
                    self.broadcaster.broadcast(message);
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
                Message::Chat(chat) => {
                    self.broadcaster.broadcast(Message::Chat(chat));
                }
                Message::PlayerLeavePlot(player_arc) => {
                    let player = Arc::try_unwrap(player_arc).unwrap();
                    let plot_x = (player.x as i32) >> 7;
                    let plot_z = (player.z as i32) >> 7;
                    let plot_loaded = self
                        .running_plots
                        .iter()
                        .any(|p| p.plot_x == plot_x && p.plot_z == plot_z);
                    self.update_player_entry(player.uuid, plot_x, plot_z);
                    if !plot_loaded {
                        let (priv_tx, priv_rx) = mpsc::channel();
                        Plot::load_and_run(
                            plot_x,
                            plot_z,
                            self.broadcaster.add_rx(),
                            self.plot_sender.clone(),
                            priv_rx,
                            false,
                            Some(player),
                        );
                        self.running_plots.push(PlotListEntry {
                            plot_x,
                            plot_z,
                            priv_message_sender: priv_tx,
                        });
                    } else {
                        let plot_list_entry = self
                            .running_plots
                            .iter()
                            .find(|p| p.plot_x == plot_x && p.plot_z == plot_z)
                            .unwrap();
                        plot_list_entry
                            .priv_message_sender
                            .send(PrivMessage::PlayerEnterPlot(player));
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
                            let handshake = S00Handshake::decode(packet).unwrap();
                            let client = &mut clients[client];
                            match handshake.next_state {
                                1 => client.state = NetworkState::Status,
                                2 => client.state = NetworkState::Login,
                                _ => {}
                            }
                            if client.state == NetworkState::Login
                                && handshake.protocol_version != 578
                            {
                                let disconnect = C00DisconnectLogin {
                                    reason: json!({
                                        "text": "Version mismatch, I'm on 1.15.2!"
                                    })
                                    .to_string(),
                                }
                                .encode();
                                client.send_packet(&disconnect);
                                client.close_connection();
                            }
                        }
                    }
                    NetworkState::Status => {
                        let client = &mut clients[client];
                        match packet.packet_id {
                            0x00 => {
                                let response = C00Response {
                                    json_response: json!({
                                        "version": {
                                            "name": "1.15.2",
                                            "protocol": 578
                                        },
                                        "players": {
                                            "max": 9999,
                                            "online": self.online_players.len(),
                                            "sample": []
                                        },
                                        "description": {
                                            "text": self.config.get_str("motd").unwrap_or_default()
                                        }
                                    })
                                    .to_string(),
                                }
                                .encode();
                                client.send_packet(&response);
                            }
                            0x01 => {
                                let ping = S00Ping::decode(packet).unwrap();
                                let pong = C01Pong {
                                    payload: ping.payload,
                                }
                                .encode();
                                client.send_packet(&pong);
                            }
                            _ => {}
                        }
                    }
                    NetworkState::Login => {
                        if packet.packet_id == 0x00 {
                            let login_start = S00LoginStart::decode(packet).unwrap();
                            clients[client].username = Some(login_start.name);
                            let set_compression = C03SetCompression { threshold: 500 }.encode();
                            clients[client].send_packet(&set_compression);
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
                            clients[client].send_packet(&login_success);

                            clients[client].state = NetworkState::Play;
                            let mut client = clients.remove(client);

                            let join_game = C26JoinGame {
                                entity_id: client.id as i32,
                                gamemode: 1,
                                dimention: 0,
                                hash_seed: 0,
                                max_players: u8::MAX,
                                level_type: "flat".to_string(),
                                view_distance: 8,
                                reduced_debug_info: false,
                                enable_respawn_screen: false,
                            }
                            .encode();
                            client.send_packet(&join_game);

                            let brand = C19PluginMessageBrand {
                                brand: "Minecraft High Performace Redstone Server".to_string(),
                            }
                            .encode();
                            client.send_packet(&brand);

                            let mut player = Player::load_player(uuid, username.clone(), client);

                            let player_pos_and_look = C36PlayerPositionAndLook {
                                x: player.x,
                                y: player.y,
                                z: player.z,
                                yaw: player.yaw,
                                pitch: player.pitch,
                                flags: 0,
                                teleport_id: 0,
                            }
                            .encode();
                            player.client.send_packet(&player_pos_and_look);

                            let mut add_player_list = Vec::new();
                            for player in &self.online_players {
                                add_player_list.push(C34PlayerInfoAddPlayer {
                                    uuid: player.uuid,
                                    name: player.username.clone(),
                                    display_name: None,
                                    gamemode: 1,
                                    ping: 0,
                                    properties: Vec::new(),
                                });
                            }
                            add_player_list.push(C34PlayerInfoAddPlayer {
                                uuid: player.uuid,
                                name: player.username.clone(),
                                display_name: None,
                                gamemode: 1,
                                ping: 0,
                                properties: Vec::new(),
                            });
                            let player_info = C34PlayerInfo::AddPlayer(add_player_list).encode();
                            player.client.send_packet(&player_info);

                            let slot_data: Vec<Option<SlotData>> = player
                                .inventory
                                .iter()
                                .map(|op| {
                                    op.as_ref().map(|item| SlotData {
                                        item_count: item.count as i8,
                                        item_id: item.id as i32,
                                        nbt: item.nbt.clone(),
                                    })
                                })
                                .collect();
                            let window_items = C15WindowItems {
                                window_id: 0,
                                slot_data,
                            }
                            .encode();
                            player.client.send_packet(&window_items);

                            self.plot_sender
                                .send(Message::PlayerJoined(Arc::new(player)))
                                .unwrap();
                        }
                    }
                    NetworkState::Play => {}
                }
            }
        }
    }
}
