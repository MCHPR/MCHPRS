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
use crate::player::Player;
use crate::plot::{self, Plot};
use backtrace::Backtrace;
use bus::{Bus, BusReader};
use fern::colors::{Color, ColoredLevelConfig};
use log::{debug, error, info, warn};
use serde_json::json;
use std::fs;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

/// Messages get passed between plot threads, the server thread, and the networking thread.
/// These messages are used to communicate when a player joins, leaves, or moves into another plot,
/// as well as to communicate chat messages.
#[derive(Debug)]
pub enum Message {
    ChatInfo(String, String),
    PlayerJoined(Player),
    PlayerLeft(u128),
    PlayerLeavePlot(Player),
    PlayerTeleportOther(Player, String),
    PlotUnload(i32, i32),
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum BroadcastMessage {
    Chat(String),
    PlayerJoinedInfo(PlayerJoinInfo),
    PlayerLeft(u128),
    Shutdown,
}

#[derive(Debug)]
pub enum PrivMessage {
    PlayerEnterPlot(Player),
    PlayerTeleportOther(Player, String),
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
    broadcaster: Bus<BroadcastMessage>,
    debug_plot_receiver: BusReader<BroadcastMessage>,
    receiver: Receiver<Message>,
    plot_sender: Sender<Message>,
    //permissions: Arc<Mutex<Permissions>>,
    online_players: Vec<PlayerListEntry>,
    running_plots: Vec<PlotListEntry>,
}

impl MinecraftServer {
    pub fn run() {
        // Setup logging
        let colors_level = ColoredLevelConfig::new()
            .info(Color::Green)
            .error(Color::Red)
            .warn(Color::Yellow);
        fern::Dispatch::new()
            .format(move |out, message, record| {
                out.finish(format_args!(
                    "[\x1B[32m{date}\x1B[0m][{target}][{level}] {message}\x1B[0m",
                    date = chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                    target = record.target(),
                    level = colors_level.color(record.level()),
                    message = message,
                ))
            })
            .level(log::LevelFilter::Debug)
            .chain(std::io::stdout())
            .chain(fern::log_file("output.log").unwrap())
            .apply()
            .unwrap();

        std::panic::set_hook(Box::new(|panic_info| {
            error!("{}", panic_info.to_string());
            let backtrace = Backtrace::new();
            for frame in backtrace.frames() {
                for symbol in frame.symbols() {
                    // TODO: Make prettier
                    error!("{:?}", symbol);
                }
            }
        }));

        info!("Starting server...");
        let start_time = Instant::now();

        // Create world folders if they don't exist yet
        fs::create_dir_all("./world/players").unwrap();
        fs::create_dir_all("./world/plots").unwrap();

        plot::database::init();

        // Load config
        let mut config = config::Config::default();
        config
            .merge(config::File::with_name("Config"))
            .expect("Error reading config file!");
        let bind_addr = config
            .get_str("bind_address")
            .expect("Bind address not found in config file!");

        //let permissions = Arc::new(Mutex::new(Permissions::new(&config)));
        // Create thread messaging structs
        let (plot_tx, server_rx) = mpsc::channel();
        let mut bus = Bus::new(100);
        let debug_plot_receiver = bus.add_rx();
        let ctrl_handler_sender = plot_tx.clone();

        ctrlc::set_handler(move || {
            ctrl_handler_sender.send(Message::Shutdown).unwrap();
        })
        .expect("There was an error setting the ctrlc handler");

        // Create server struct
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

        info!("Done! Start took {:?}", start_time.elapsed());

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

    fn handle_plot_unload(&mut self, plot_x: i32, plot_z: i32) {
        let index = self
            .running_plots
            .iter()
            .position(|p| p.plot_x == plot_x && p.plot_z == plot_z);
        if let Some(index) = index {
            self.running_plots.remove(index);
        }
    }

    fn graceful_shutdown(&mut self) {
        info!("Commencing graceful shutdown...");
        self.broadcaster.broadcast(BroadcastMessage::Shutdown);
        // Wait for all plots to save and unload
        while !self.running_plots.is_empty() {
            while let Ok(message) = self.receiver.try_recv() {
                if let Message::PlotUnload(plot_x, plot_z) = message {
                    self.handle_plot_unload(plot_x, plot_z);
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        std::process::exit(0);
    }

    fn send_player_to_plot(&mut self, player: Player, new_entry: bool) {
        let plot_x = (player.x as i32) >> 8;
        let plot_z = (player.z as i32) >> 8;

        if new_entry {
            let player_list_entry = PlayerListEntry {
                plot_x,
                plot_z,
                username: player.username.clone(),
                uuid: player.uuid,
                skin: None,
            };
            self.online_players.push(player_list_entry);
        } else {
            self.update_player_entry(player.uuid, plot_x, plot_z);
        }

        let plot_loaded = self
            .running_plots
            .iter()
            .any(|p| p.plot_x == plot_x && p.plot_z == plot_z);
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

    fn update(&mut self) {
        while let Ok(message) = self.debug_plot_receiver.try_recv() {
            debug!("Main thread broadcasted message: {:#?}", message);
        }
        while let Ok(message) = self.receiver.try_recv() {
            debug!("Main thread received message: {:#?}", message);
            match message {
                Message::PlayerJoined(player) => {
                    // Send player info to plots
                    let player_join_info = PlayerJoinInfo {
                        username: player.username.clone(),
                        uuid: player.uuid,
                        skin: None,
                    };
                    self.broadcaster
                        .broadcast(BroadcastMessage::PlayerJoinedInfo(player_join_info));
                    self.send_player_to_plot(player, true);
                }
                Message::PlayerLeft(uuid) => {
                    let index = self.online_players.iter().position(|p| p.uuid == uuid);
                    if let Some(index) = index {
                        self.online_players.remove(index);
                    }
                    self.broadcaster
                        .broadcast(BroadcastMessage::PlayerLeft(uuid));
                }
                Message::PlotUnload(plot_x, plot_z) => self.handle_plot_unload(plot_x, plot_z),
                Message::ChatInfo(username, message) => {
                    self.broadcaster.broadcast(BroadcastMessage::Chat(
                        json!({
                            "text": self.config.get_str("chat_format").unwrap_or("<{username}> {message}".to_owned())
                                        .replace("{username}", &username)
                                        .replace("{message}", &message)
                        }).to_string()
                    ));
                }
                Message::PlayerLeavePlot(player) => {
                    self.send_player_to_plot(player, false);
                }
                Message::Shutdown => {
                    self.graceful_shutdown();
                }
                Message::PlayerTeleportOther(mut player, other_username) => {
                    if let Some(other_player) = self
                        .online_players
                        .iter()
                        .find(|p| p.username == other_username)
                    {
                        let plot_x = other_player.plot_x;
                        let plot_z = other_player.plot_z;

                        let plot_loaded = self
                            .running_plots
                            .iter()
                            .any(|p| p.plot_x == plot_x && p.plot_z == plot_z);
                        if !plot_loaded {
                            player.send_system_message(
                                "Their plot wasn't loaded. How did this happen??",
                            );
                            self.send_player_to_plot(player, false);
                        } else {
                            self.update_player_entry(player.uuid, plot_x, plot_z);
                            let plot_list_entry = self
                                .running_plots
                                .iter()
                                .find(|p| p.plot_x == plot_x && p.plot_z == plot_z)
                                .unwrap();
                            plot_list_entry
                                .priv_message_sender
                                .send(PrivMessage::PlayerTeleportOther(player, other_username));
                        }
                    } else {
                        player.send_system_message("Player not found!");
                        self.send_player_to_plot(player, false);
                    }
                }
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
                                warn!("A player tried to connect using the wrong version");
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
                                            "max": self.config.get_int("max_players").unwrap_or_default(),
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
                                max_players: 0,
                                level_type: "flat".to_string(),
                                view_distance: 8,
                                reduced_debug_info: false,
                                enable_respawn_screen: false,
                            }
                            .encode();
                            client.send_packet(&join_game);

                            let brand = C19PluginMessageBrand {
                                brand: "Minecraft High Performace Redstone".to_string(),
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

                            player.update_view_pos();

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
                                        item_id: item.item_type.get_id() as i32,
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
                                .send(Message::PlayerJoined(player))
                                .unwrap();
                        }
                    }
                    NetworkState::Play => {}
                }
            }
        }
    }
}
