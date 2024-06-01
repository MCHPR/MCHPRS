use crate::config::CONFIG;
use crate::player::{Gamemode, PacketSender, Player};
use crate::plot::commands::DECLARE_COMMANDS;
use crate::plot::{self, database, Plot, PLOT_BLOCK_HEIGHT};
use crate::utils::HyphenatedUUID;
use crate::{permissions, utils};
use backtrace::Backtrace;
use bus::Bus;
use mchprs_network::packets::clientbound::{
    CConfigurationPluginMessage, CDisconnectLogin, CFinishConfiguration, CGameEvent, CGameEventType, CLogin, CLoginSuccess, CPlayerInfoActions, CPlayerInfoAddPlayer, CPlayerInfoUpdate, CPlayerInfoUpdatePlayer, CPong, CRegistryBiome, CRegistryBiomeEffects, CRegistryData, CRegistryDataCodec, CRegistryDimension, CResponse, CSetCompression, CSetContainerContent, CSetHeldItem, CSynchronizePlayerPosition, ClientBoundPacket, UpdateTime
};
use mchprs_network::packets::serverbound::{
    SAcknowledgeFinishConfiguration, SHandshake, SLoginAcknowledged, SLoginStart, SPing, SRequest,
    ServerBoundPacketHandler,
};
use mchprs_network::packets::{PacketEncoderExt, SlotData, COMPRESSION_THRESHOLD};
use mchprs_network::{NetworkServer, NetworkState, PlayerPacketSender};
use mchprs_text::TextComponent;
use mchprs_utils::map;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs::{self, File};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

pub const MC_VERSION: &str = "1.20.4";
pub const MC_DATA_VERSION: i32 = 3700;
pub const PROTOCOL_VERSION: i32 = 765;

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
    /// This message is sent to the server thread when a player runs /whitelist add.
    WhitelistAdd(u128, String, PlayerPacketSender),
    /// This message is sent to the server thread when a player runs /whitelist remove.
    WhitelistRemove(u128, PlayerPacketSender),
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
    Chat(u128, Vec<TextComponent>),
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
    pub gamemode: Gamemode,
}

#[derive(Debug, Clone)]
struct PlayerListEntry {
    plot_x: i32,
    plot_z: i32,
    username: String,
    gamemode: Gamemode,
}

struct PlotListEntry {
    plot_x: i32,
    plot_z: i32,
    priv_message_sender: mpsc::Sender<PrivMessage>,
}

#[derive(Serialize, Deserialize)]
struct WhitelistEntry {
    uuid: HyphenatedUUID,
    name: String,
}

/// This represents a minecraft server
pub struct MinecraftServer {
    network: NetworkServer,
    broadcaster: Bus<BroadcastMessage>,
    receiver: Receiver<Message>,
    plot_sender: Sender<Message>,
    online_players: FxHashMap<u128, PlayerListEntry>,
    running_plots: Vec<PlotListEntry>,
    whitelist: Option<Vec<WhitelistEntry>>,
}

impl MinecraftServer {
    /// Start the server
    pub fn run() {
        std::panic::set_hook(Box::new(|panic_info| {
            let backtrace = Backtrace::new();
            error!("plot {}\n{:?}", panic_info.to_string(), backtrace);
        }));

        info!("Starting server...");
        let start_time = Instant::now();

        // Create world folders if they don't exist yet
        fs::create_dir_all("./world/players").unwrap();
        fs::create_dir_all("./world/plots").unwrap();
        fs::create_dir_all("./schems").unwrap();

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

        let whitelist = CONFIG.whitelist.then(|| {
            if !Path::new("whitelist.json").exists() {
                File::create("whitelist.json").expect("Failed to create whitelist.json");
            }
            serde_json::from_reader(
                File::open("whitelist.json").expect("Failed to open whitelist.json"),
            )
            .unwrap_or_default()
        });

        if let Some(permissions_config) = &CONFIG.luckperms {
            permissions::init(permissions_config.clone()).unwrap();
        }

        // Create server struct
        let mut server = MinecraftServer {
            network: NetworkServer::new(bind_addr),
            broadcaster: bus,
            receiver: server_rx,
            plot_sender: plot_tx,
            online_players: FxHashMap::default(),
            running_plots: Vec::new(),
            whitelist,
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

        if let Some(whitelist) = &self.whitelist {
            fs::write("whitelist.json", serde_json::to_string(whitelist).unwrap()).unwrap();
        }

        std::process::exit(0);
    }

    fn send_player_to_plot(&mut self, player: Player, new_entry: bool) {
        let (plot_x, plot_z) = player.pos.plot_pos();

        if new_entry {
            let player_list_entry = PlayerListEntry {
                plot_x,
                plot_z,
                username: player.username.clone(),
                gamemode: player.gamemode,
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

    fn handle_player_enter_play(&mut self, client_idx: usize) {
        let client = self.network.handshaking_clients.remove(client_idx);

        let uuid = client.uuid.clone().unwrap();
        let username = client.username.clone().unwrap();
        let player = Player::load_player(uuid, username, client.into());

        let join_game = CLogin {
            entity_id: player.entity_id as i32,
            is_hardcore: false,
            dimension_names: vec!["mchprs:world".to_owned()],
            max_players: 0,
            view_distance: CONFIG.view_distance as i32,
            simulation_distance: CONFIG.view_distance as i32,
            reduced_debug_info: false,
            enable_respawn_screen: false,
            do_limited_crafting: false,
            dimension_type: "mchprs:dimension".to_owned(),
            dimension_name: "mchprs:plotworld".to_owned(),
            hashed_seed: 0,
            gamemode: player.gamemode.get_id() as u8,
            previous_gamemode: 1,
            is_debug: false,
            is_flat: true,
            death_location: None,
            portal_cooldown: 0,
        }
        .encode();
        player.client.send_packet(&join_game);

        // Send the player's position and rotation.
        let player_pos_and_look = CSynchronizePlayerPosition {
            x: player.pos.x,
            y: player.pos.y,
            z: player.pos.z,
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
        for (&uuid, player) in &self.online_players {
            let mut actions: CPlayerInfoActions = Default::default();
            actions.add_player = Some(CPlayerInfoAddPlayer {
                name: player.username.clone(),
                properties: Vec::new(),
            });
            actions.update_gamemode = Some(player.gamemode.get_id());
            add_player_list.push(CPlayerInfoUpdatePlayer {
                uuid,
                actions,
            });
        }
        add_player_list.push({
            let mut actions: CPlayerInfoActions = Default::default();
            actions.add_player = Some(CPlayerInfoAddPlayer {
                name: player.username.clone(),
                properties: Vec::new(),
            });
            actions.update_gamemode = Some(player.gamemode.get_id());
            CPlayerInfoUpdatePlayer {
                uuid: player.uuid,
                actions
            }
        });

        let player_info = CPlayerInfoUpdate {
            players: add_player_list,
        }.encode();
        player.client.send_packet(&player_info);

        // Send the player's inventory
        let slot_data: Vec<Option<SlotData>> = player
            .inventory
            .iter()
            .map(|op| op.as_ref().map(|item| utils::encode_slot_data(item)))
            .collect();
        let window_items = CSetContainerContent {
            window_id: 0,
            state_id: 0,
            slot_data,
            carried_item: None,
        }
        .encode();
        player.client.send_packet(&window_items);

        // Send the player's selected item slot
        let held_item_change = CSetHeldItem {
            slot: player.selected_slot as i8,
        }
        .encode();
        player.client.send_packet(&held_item_change);

        player.client.send_packet(&DECLARE_COMMANDS);

        let time_update = UpdateTime {
            world_age: 0,
            // Noon
            time_of_day: -6000,
        }
        .encode();
        player.client.send_packet(&time_update);

        player.update_player_abilities();

        let game_event = CGameEvent {
            reason: CGameEventType::WaitForChunks,
            value: 0.0,
        }.encode();
        player.client.send_packet(&game_event);

        self.plot_sender
            .send(Message::PlayerJoined(player))
            .unwrap();
    }

    fn handle_player_login(&mut self, client_idx: usize, login_start: SLoginStart) {
        let clients = &mut self.network.handshaking_clients;
        let username = login_start.name;
        clients[client_idx].username = Some(username.clone());
        let set_compression = CSetCompression {
            threshold: COMPRESSION_THRESHOLD as i32,
        }
        .encode();
        clients[client_idx].send_packet(&set_compression);
        clients[client_idx].set_compressed(true);

        if let Some(whitelist) = &self.whitelist {
            // uuid will only be present if bungeecord is enabled in config
            let whitelisted = if let Some(uuid) = clients[client_idx].uuid {
                whitelist.iter().any(|entry| entry.uuid.0 == uuid)
            } else {
                whitelist.iter().any(|entry| entry.name == username)
            };
            if !whitelisted {
                let disconnect = CDisconnectLogin {
                    reason: json!({
                        "text": "You are not whitelisted on this server"
                    })
                    .to_string(),
                }
                .encode();
                clients[client_idx].send_packet(&disconnect);
            }
        }

        let uuid = clients[client_idx]
            .uuid
            .unwrap_or_else(|| Player::generate_offline_uuid(&username));
        clients[client_idx].uuid = Some(uuid);

        let login_success = CLoginSuccess {
            uuid,
            username: username.clone(),
            // TODO: send player properties
            properties: Vec::new(),
        }
        .encode();
        clients[client_idx].send_packet(&login_success);
    }

    fn handle_message(&mut self, message: Message) {
        match message {
            Message::PlayerJoined(player) => {
                info!("{} joined the game", player.username);
                // Send player info to plots
                let player_join_info = PlayerJoinInfo {
                    username: player.username.clone(),
                    uuid: player.uuid,
                    gamemode: player.gamemode,
                };
                database::ensure_user(&format!("{:032x}", player.uuid), &player.username);
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
                    TextComponent::from_legacy_text(
                        &CONFIG
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
            Message::PlayerTeleportOther(player, other_username) => {
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
            Message::WhitelistAdd(uuid, username, sender) => {
                if let Some(whitelist) = &mut self.whitelist {
                    let msg = format!("{} was sucessfully added to the whitelist.", &username);
                    sender.send_system_message(&msg);
                    let uuid = HyphenatedUUID(uuid);
                    debug!("Added to whitelist: {} ({})", &username, uuid.to_string());

                    whitelist.push(WhitelistEntry {
                        name: username,
                        uuid,
                    });
                } else {
                    sender.send_error_message("Whitelist is not enabled!");
                }
            }
            Message::WhitelistRemove(uuid, sender) => {
                if let Some(whitelist) = &mut self.whitelist {
                    let mut found = false;
                    whitelist.retain(|entry| {
                        let matches = entry.uuid.0 == uuid;
                        if matches {
                            let msg = format!(
                                "{} was sucessfully removed from the whitelist.",
                                &entry.name
                            );
                            sender.send_system_message(&msg);
                            debug!(
                                "Removed from whitelist: {}",
                                HyphenatedUUID(uuid).to_string()
                            );
                            found = true;
                        }
                        !matches
                    });
                    if !found {
                        sender.send_error_message("That player is not whitelisted on this server.");
                    }
                } else {
                    sender.send_error_message("Whitelist is not enabled!");
                }
            }
        }
    }

    fn update(&mut self) {
        while let Ok(message) = self.receiver.try_recv() {
            self.handle_message(message);
        }
        self.network.update();

        let mut client_idx = 0;
        let mut clients_len = self.network.handshaking_clients.len();
        loop {
            if client_idx >= clients_len {
                break;
            }

            let packets = self.network.handshaking_clients[client_idx].receive_packets();
            for packet in packets {
                packet.handle(self, client_idx);
            }

            let new_len = self.network.handshaking_clients.len();

            if clients_len == new_len {
                client_idx += 1;
            }
            clients_len = new_len;
        }
    }
}

impl ServerBoundPacketHandler for MinecraftServer {
    fn handle_handshake(&mut self, handshake: SHandshake, client_idx: usize) {
        let clients = &mut self.network.handshaking_clients;
        let client = &mut clients[client_idx];
        let next_state = match handshake.next_state {
            1 => NetworkState::Status,
            2 => NetworkState::Login,
            // TODO: Handle invalid next state
            _ => return,
        };
        if next_state == NetworkState::Login && handshake.protocol_version != PROTOCOL_VERSION {
            warn!("A player tried to connect using the wrong version");
            let disconnect = CDisconnectLogin {
                reason: json!({ "text": format!("Version mismatch, I'm on {}!", MC_VERSION) })
                    .to_string(),
            }
            .encode();
            client.send_packet(&disconnect);
            client.close_connection();
        } else if next_state == NetworkState::Login && CONFIG.bungeecord {
            let split: Vec<&str> = handshake.server_address.split('\u{0}').collect();
            if split.len() == 3 || split.len() == 4 {
                client.uuid = u128::from_str_radix(split[2], 16).ok();
            } else {
                let disconnect = CDisconnectLogin {
                    reason: json!({
                        "text": "If you wish to use IP forwarding, please enable it in your BungeeCord config as well!"
                    })
                    .to_string(),
                }
                .encode();
                client.send_packet(&disconnect);
                client.close_connection();
            }
        }
    }

    fn handle_request(&mut self, _request: SRequest, client_idk: usize) {
        let client = &mut self.network.handshaking_clients[client_idk];
        let response = CResponse {
            json_response: json!({
                "version": {
                    "name": MC_VERSION,
                    "protocol": PROTOCOL_VERSION
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

    fn handle_ping(&mut self, ping: SPing, client_idx: usize) {
        let client = &mut self.network.handshaking_clients[client_idx];
        let pong = CPong {
            payload: ping.payload,
        }
        .encode();
        client.send_packet(&pong);
    }

    fn handle_login_start(&mut self, login_start: SLoginStart, client_idx: usize) {
        self.handle_player_login(client_idx, login_start);
    }

    fn handle_login_acknowledged(
        &mut self,
        _login_acknowledged: SLoginAcknowledged,
        client_idx: usize,
    ) {
        // The client has been switched to the Configuration state
        let client = &mut self.network.handshaking_clients[client_idx];

        // Sends the custom brand name to the player
        // (This can be seen in the f3 debug menu in-game)
        let brand = CConfigurationPluginMessage {
            channel: String::from("minecraft:brand"),
            data: {
                let mut data = Vec::new();
                data.write_string(32767, "Minecraft High Performance Redstone");
                data
            },
        }
        .encode();
        client.send_packet(&brand);

        let dimension = CRegistryDimension {
            fixed_time: Some(6000),
            has_skylight: true,
            has_ceiling: false,
            ultrawarm: false,
            natural: true,
            coordinate_scale: 1.0,
            bed_works: false,
            respawn_anchor_works: false,
            min_y: 0,
            height: PLOT_BLOCK_HEIGHT,
            logical_height: PLOT_BLOCK_HEIGHT,
            infiniburn: "#minecraft:infiniburn_overworld".to_owned(),
            effects: "#minecraft:overworld".to_owned(),
            ambient_light: 1.0,
            piglin_safe: false,
            has_raids: false,
            monster_spawn_block_light_limit: 0,
            monster_spawn_light_level: 0,
        };

        let codec = CRegistryDataCodec {
            dimensions: map! {
                "mchprs:dimension" => dimension.clone()
            },
            biomes: map! {
                "mchprs:plot" => CRegistryBiome {
                    has_precipitation: false,
                    temperature: 0.5,
                    downfall: 0.5,
                    effects: CRegistryBiomeEffects {
                        sky_color: 0x7BA4FF,
                        water_fog_color: 0x050533,
                        fog_color: 0xC0D8FF,
                        water_color: 0x3F76E4,
                    },
                }
                // Apparently the client NEEDS this to exist
                // "minecraft:plains" => CRegistryBiome {
                //     precipitation: "none".to_owned(),
                //     effects: CRegistryBiomeEffects {
                //         sky_color: 7907327,
                //         water_fog_color: 329011,
                //         fog_color: 12638463,
                //         water_color: 4159204,
                //         mood_sound: CJoinGameBiomeEffectsMoodSound {
                //             tick_delay: 6000,
                //             offset: 2.0,
                //             sound: "minecraft:ambient.cave".to_owned(),
                //             block_search_extent: 8,
                //         }
                //     },
                //     depth: 0.125,
                //     temperature: 0.8,
                //     scale: 0.5,
                //     downfall: 0.4,
                //     category: "none".to_owned(),
                // }
            },
        };
        let registry_data = CRegistryData {
            registry_codec: codec,
        };
        client.send_packet(&registry_data.encode());

        client.send_packet(&CFinishConfiguration.encode());
    }

    fn handle_acknowledge_finish_configuration(
        &mut self,
        _ackowledge_finish_configuration: SAcknowledgeFinishConfiguration,
        client_idx: usize,
    ) {
        self.handle_player_enter_play(client_idx);
    }
}
