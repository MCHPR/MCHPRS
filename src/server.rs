use crate::chat::ChatComponent;
use crate::config::CONFIG;
use crate::network::packets::clientbound::{
    C00DisconnectLogin, C00Response, C01Pong, C02LoginSuccess, C03SetCompression, C13WindowItems,
    C17PluginMessage, C24JoinGame, C24JoinGameBiomeEffects, C24JoinGameBiomeEffectsMoodSound,
    C24JoinGameBiomeElement, C24JoinGameDimensionCodec, C24JoinGameDimensionElement, C32PlayerInfo,
    C32PlayerInfoAddPlayer, C34PlayerPositionAndLook, C3FHeldItemChange, C4ETimeUpdate,
    ClientBoundPacket,
};
use crate::network::packets::serverbound::{
    S00Handshake, S00LoginStart, S00Request, S01Ping, ServerBoundPacketHandler,
};
use crate::network::packets::{PacketEncoderExt, SlotData};
use crate::network::{NetworkServer, NetworkState};
use crate::player::{Gamemode, Player};
use crate::plot::{self, commands::DECLARE_COMMANDS, database, Plot};
use backtrace::Backtrace;
use bus::Bus;
use fern::colors::{Color, ColoredLevelConfig};
use log::{error, info, warn};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

/// `Message` gets send from a plot thread to the server thread.
#[derive(Debug)]
pub enum Message {
    /// This message is sent to the server thread when a player sends a chat message,
    /// It contains the uuid and name of the player and the raw message the player sent.
    ChatInfo(u128, String, String),
    /// This message is sent to the server thread when a player joins the server.
    PlayerJoined(Player),
    /// This message is sent to the server thread when a player leaves the server.
    PlayerLeft(u128),
    /// This message is sent to the server thread when a player goes outside of their plot.
    PlayerLeavePlot(Player),
    /// This message is sent to the server thread when a player runs /tp <name>.
    PlayerTeleportOther(Player, String),
    /// This message is sent to the server thread when a player changes their gamemode.
    PlayerUpdateGamemode(u128, Gamemode),
    /// This message is sent to the server thread when a plot unloads itself.
    PlotUnload(i32, i32),
    /// This message is sent to the server thread when a player runs /stop.
    Shutdown,
}

/// `BroadcastMessage` gets broadcasted from the server thread to all the plot threads.
/// This happens when there is a chat message, a player joins or leaves, or the server
/// shuts down.
#[derive(Debug, Clone)]
pub enum BroadcastMessage {
    /// This message is broadcasted for chat messages. It contains the uuid of the player and
    /// the raw json data to send to the clients.
    Chat(u128, Vec<ChatComponent>),
    /// This message is broadcasted when a player joins the server. It is used to update
    /// the tab-list on all connected clients.
    PlayerJoinedInfo(PlayerJoinInfo),
    /// This message is broadcasted when a player leaves the server. It is used to update
    /// the tab-list on all connected clients.
    PlayerLeft(u128),
    /// This message is broadcasted when a player changes their gamemode,
    PlayerUpdateGamemode(u128, Gamemode),
    /// This message is broadcasted when the server is stopping, either through the stop
    /// command or through the ctrl+c handler.
    Shutdown,
}

/// `PrivMessage` gets send from the server thread directly to a plot thread.
/// This only happens when a player is getting transfered to a plot.
#[derive(Debug)]
pub enum PrivMessage {
    PlayerEnterPlot(Player),
    PlayerTeleportOther(Player, String),
}

/// This is the data that gets sent in the `PlayerJoinedInfo` broadcast message.
/// It contains imformation such as the player's username, uuid, and skin.
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
    gamemode: Gamemode,
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
    broadcaster: Bus<BroadcastMessage>,
    receiver: Receiver<Message>,
    plot_sender: Sender<Message>,
    online_players: HashMap<u128, PlayerListEntry>,
    running_plots: Vec<PlotListEntry>,
}

impl MinecraftServer {
    /// Setup logging, set the panic hook,
    /// create the world if it does not exist,
    /// and then finally start the server.
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
            error!("{}\n{:?}", panic_info.to_string(), backtrace);
        }));

        info!("Starting server...");
        let start_time = Instant::now();

        // Create world folders if they don't exist yet
        fs::create_dir_all("./world/players").unwrap();
        fs::create_dir_all("./world/plots").unwrap();

        plot::database::init();

        let bind_addr = CONFIG.bind_address.clone();

        // Create thread messaging structs
        let (plot_tx, server_rx) = mpsc::channel();
        let bus = Bus::new(100);
        let ctrl_handler_sender = plot_tx.clone();

        ctrlc::set_handler(move || {
            ctrl_handler_sender.send(Message::Shutdown).unwrap();
        })
        .expect("There was an error setting the ctrlc handler");

        // Create server struct
        let mut server = MinecraftServer {
            network: NetworkServer::new(bind_addr),
            broadcaster: bus,
            receiver: server_rx,
            plot_sender: plot_tx,
            online_players: HashMap::new(),
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

    /// Updates the player's location on the `online_players` list
    fn update_player_entry(&mut self, uuid: u128, plot_x: i32, plot_z: i32) {
        let player = self.online_players.get_mut(&uuid);
        if let Some(player) = player {
            player.plot_x = plot_x;
            player.plot_z = plot_z;
        }
    }

    /// Removes the plot entry from the `running_plots` list
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
                gamemode: player.gamemode,
                uuid: player.uuid,
                skin: None,
            };
            self.online_players.insert(player.uuid, player_list_entry);
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
            let _ = plot_list_entry
                .priv_message_sender
                .send(PrivMessage::PlayerEnterPlot(player));
        }
    }

    fn handle_player_login(&mut self, client_idx: usize, login_start: S00LoginStart) {
        let clients = &mut self.network.handshaking_clients;
        clients[client_idx].username = Some(login_start.name);
        let set_compression = C03SetCompression { threshold: 256 }.encode();
        clients[client_idx].send_packet(&set_compression);
        clients[client_idx].set_compressed(true);
        let username = if let Some(name) = &clients[client_idx].username {
            name.clone()
        } else {
            Default::default()
        };
        let uuid = clients[client_idx]
            .uuid
            .unwrap_or_else(|| Player::generate_offline_uuid(&username));

        let login_success = C02LoginSuccess {
            uuid,
            username: username.clone(),
        }
        .encode();
        clients[client_idx].send_packet(&login_success);

        clients[client_idx].state = NetworkState::Play;
        let client = clients.remove(client_idx);

        let mut player = Player::load_player(uuid, username, client);

        let join_game = C24JoinGame {
            entity_id: player.client.id as i32,
            is_hardcore: false,
            gamemode: player.gamemode.get_id() as u8,
            previous_gamemode: 1,
            world_count: 1,
            world_names: vec!["mchprs:world".to_owned()],
            dimension_codec: C24JoinGameDimensionCodec {
                dimensions: map! {
                    "mchprs:world".to_owned() => C24JoinGameDimensionElement {
                        natural: 1,
                        ambient_light: 1.0,
                        has_ceiling: 0,
                        has_skylight: 1,
                        fixed_time: 6000,
                        shrunk: 0,
                        ultrawarm: 0,
                        has_raids: 0,
                        respawn_anchor_works: 0,
                        bed_works: 0,
                        coordinate_scale: 1.0,
                        piglin_safe: 0,
                        logical_height: 256,
                        infiniburn: "".to_owned(),
                    }
                },
                biomes: map! {
                    "mchprs:plot".to_owned() => C24JoinGameBiomeElement {
                        precipitation: "none".to_owned(),
                        effects: C24JoinGameBiomeEffects {
                            sky_color: 0x7BA4FF,
                            water_fog_color: 0x050533,
                            fog_color: 0xC0D8FF,
                            water_color: 0x3F76E4,
                            mood_sound: C24JoinGameBiomeEffectsMoodSound {
                                tick_delay: 6000,
                                offset: 2.0,
                                sound: "minecraft:ambient.cave".to_owned(),
                                block_search_extent: 8,
                            }
                        },
                        depth: 0.1,
                        temperature: 0.5,
                        scale: 0.2,
                        downfall: 0.5,
                        category: "none".to_owned(),
                    },
                    "minecraft:plains".to_owned() => C24JoinGameBiomeElement {
                        precipitation: "none".to_owned(),
                        effects: C24JoinGameBiomeEffects {
                            sky_color: 7907327,
                            water_fog_color: 329011,
                            fog_color: 12638463,
                            water_color: 4159204,
                            mood_sound: C24JoinGameBiomeEffectsMoodSound {
                                tick_delay: 6000,
                                offset: 2.0,
                                sound: "minecraft:ambient.cave".to_owned(),
                                block_search_extent: 8,
                            }
                        },
                        depth: 0.125,
                        temperature: 0.8,
                        scale: 0.5,
                        downfall: 0.4,
                        category: "none".to_owned(),
                    }
                },
            },
            // this should be exactly the same has the dimension listed in dimension_codec
            dimension: C24JoinGameDimensionElement {
                natural: 1,
                ambient_light: 1.0,
                has_ceiling: 0,
                has_skylight: 1,
                fixed_time: 6000,
                shrunk: 0,
                ultrawarm: 0,
                has_raids: 0,
                respawn_anchor_works: 0,
                bed_works: 0,
                coordinate_scale: 1.0,
                piglin_safe: 0,
                logical_height: 256,
                infiniburn: "".to_owned(),
            },
            world_name: "mchprs:world".to_owned(),
            hashed_seed: 0,
            max_players: 0,
            view_distance: 8,
            reduced_debug_info: false,
            enable_respawn_screen: false,
            is_debug: false,
            is_flat: true,
        }
        .encode();
        player.client.send_packet(&join_game);

        // Sends the custom brand name to the player
        // (This can be seen in the f3 debug menu in-game)
        let brand = C17PluginMessage {
            channel: String::from("minecraft:brand"),
            data: {
                let mut data = Vec::new();
                data.write_string(32767, "Minecraft High Performance Redstone");
                data
            },
        }
        .encode();
        player.client.send_packet(&brand);

        // Send the player's position and rotation.
        let player_pos_and_look = C34PlayerPositionAndLook {
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

        // Send the player list to the newly connected player.
        // (This is the list you see when you press tab in-game)
        let mut add_player_list = Vec::new();
        for (uuid, player) in &self.online_players {
            add_player_list.push(C32PlayerInfoAddPlayer {
                uuid: *uuid,
                name: player.username.clone(),
                display_name: None,
                gamemode: player.gamemode.get_id() as i32,
                ping: 0,
                properties: Vec::new(),
            });
        }
        add_player_list.push(C32PlayerInfoAddPlayer {
            uuid: player.uuid,
            name: player.username.clone(),
            display_name: None,
            gamemode: player.gamemode.get_id() as i32,
            ping: 0,
            properties: Vec::new(),
        });
        let player_info = C32PlayerInfo::AddPlayer(add_player_list).encode();
        player.client.send_packet(&player_info);

        // Send the player's inventory
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
        let window_items = C13WindowItems {
            window_id: 0,
            slot_data,
        }
        .encode();
        player.client.send_packet(&window_items);

        // Send the player's selected item slot
        let held_item_change = C3FHeldItemChange {
            slot: player.selected_slot as i8,
        }
        .encode();
        player.client.send_packet(&held_item_change);

        player.client.send_packet(&DECLARE_COMMANDS);

        let time_update = C4ETimeUpdate {
            world_age: 0,
            // Noon
            time_of_day: -6000,
        }
        .encode();
        player.client.send_packet(&time_update);

        player.update_player_abilities();

        self.plot_sender
            .send(Message::PlayerJoined(player))
            .unwrap();
    }

    fn handle_message(&mut self, message: Message) {
        match message {
            Message::PlayerJoined(player) => {
                info!("{} joined the game", player.username);
                // Send player info to plots
                let player_join_info = PlayerJoinInfo {
                    username: player.username.clone(),
                    uuid: player.uuid,
                    skin: None,
                };
                database::ensure_user(format!("{:032x}", player.uuid), &player.username);
                self.broadcaster
                    .broadcast(BroadcastMessage::PlayerJoinedInfo(player_join_info));
                self.send_player_to_plot(player, true);
            }
            Message::PlayerLeft(uuid) => {
                if let Some((_, player)) = self.online_players.remove_entry(&uuid) {
                    info!("{} left the game", player.username);
                }
                self.broadcaster
                    .broadcast(BroadcastMessage::PlayerLeft(uuid));
            }
            Message::PlotUnload(plot_x, plot_z) => self.handle_plot_unload(plot_x, plot_z),
            Message::ChatInfo(uuid, username, message) => {
                info!("<{}> {}", username, message);
                self.broadcaster.broadcast(BroadcastMessage::Chat(
                    uuid,
                    ChatComponent::from_legacy_text(
                        CONFIG
                            .chat_format
                            .replace("{username}", &username)
                            .replace("{message}", &message),
                    ),
                ));
            }
            Message::PlayerLeavePlot(player) => {
                self.send_player_to_plot(player, false);
            }
            Message::Shutdown => {
                self.graceful_shutdown();
            }
            Message::PlayerTeleportOther(mut player, other_username) => {
                let username_lower = other_username.to_lowercase();
                if let Some((_, other_player)) = self
                    .online_players
                    .iter()
                    .find(|(_, p)| p.username.to_lowercase().starts_with(&username_lower))
                {
                    let plot_x = other_player.plot_x;
                    let plot_z = other_player.plot_z;

                    let plot_loaded = self
                        .running_plots
                        .iter()
                        .any(|p| p.plot_x == plot_x && p.plot_z == plot_z);
                    if !plot_loaded {
                        player
                            .send_system_message("Their plot wasn't loaded. How did this happen??");
                        self.send_player_to_plot(player, false);
                    } else {
                        self.update_player_entry(player.uuid, plot_x, plot_z);
                        let plot_list_entry = self
                            .running_plots
                            .iter()
                            .find(|p| p.plot_x == plot_x && p.plot_z == plot_z)
                            .unwrap();
                        let _ = plot_list_entry
                            .priv_message_sender
                            .send(PrivMessage::PlayerTeleportOther(player, other_username));
                    }
                } else {
                    player.send_system_message("Player not found!");
                    self.send_player_to_plot(player, false);
                }
            }
            Message::PlayerUpdateGamemode(uuid, gamemode) => {
                if let Some(player) = self.online_players.get_mut(&uuid) {
                    player.gamemode = gamemode;
                }
                self.broadcaster
                    .broadcast(BroadcastMessage::PlayerUpdateGamemode(uuid, gamemode));
            }
        }
    }

    fn update(&mut self) {
        while let Ok(message) = self.receiver.try_recv() {
            self.handle_message(message);
        }
        self.network.update();
        for client in 0..self.network.handshaking_clients.len() {
            let packets = self.network.handshaking_clients[client].receive_packets();
            for packet in packets {
                packet.handle(self, client);
            }
        }
    }
}

impl ServerBoundPacketHandler for MinecraftServer {
    fn handle_handshake(&mut self, handshake: S00Handshake, client_idx: usize) {
        let clients = &mut self.network.handshaking_clients;
        let client = &mut clients[client_idx];
        match handshake.next_state {
            1 => client.state = NetworkState::Status,
            2 => client.state = NetworkState::Login,
            _ => {}
        }
        if client.state == NetworkState::Login && handshake.protocol_version != 754 {
            warn!("A player tried to connect using the wrong version");
            let disconnect = C00DisconnectLogin {
                reason: json!({
                    "text": "Version mismatch, I'm on 1.16.4!"
                })
                .to_string(),
            }
            .encode();
            client.send_packet(&disconnect);
            client.close_connection();
        } else if client.state == NetworkState::Login && CONFIG.bungeecord {
            let split: Vec<&str> = handshake.server_address.split('\u{0}').collect();
            if split.len() == 3 || split.len() == 4 {
                client.uuid = u128::from_str_radix(split[2], 16).ok();
            } else {
                let disconnect = C00DisconnectLogin {
                    reason: json!({
                        "text": "If you wish to use IP forwarding, please enable it in your BungeeCord config as well!"
                    })
                    .to_string(),
                }
                .encode();
                client.send_packet(&disconnect);
                client.close_connection();
                return;
            }
        }
    }

    fn handle_request(&mut self, _request: S00Request, client_idk: usize) {
        let client = &mut self.network.handshaking_clients[client_idk];
        let response = C00Response {
            json_response: json!({
                "version": {
                    "name": "1.16.4",
                    "protocol": 754
                },
                "players": {
                    "max": CONFIG.max_players,
                    "online": self.online_players.len(),
                    "sample": []
                },
                "description": {
                    "text": CONFIG.motd
                }
            })
            .to_string(),
        }
        .encode();
        client.send_packet(&response);
    }

    fn handle_ping(&mut self, ping: S01Ping, client_idx: usize) {
        let client = &mut self.network.handshaking_clients[client_idx];
        let pong = C01Pong {
            payload: ping.payload,
        }
        .encode();
        client.send_packet(&pong);
    }

    fn handle_login_start(&mut self, login_start: S00LoginStart, client_idx: usize) {
        self.handle_player_login(client_idx, login_start);
    }
}
