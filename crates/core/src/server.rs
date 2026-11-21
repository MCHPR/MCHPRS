use crate::config::CONFIG;
use crate::player::{Gamemode, LoadPlayerInfo, PacketSender, Player};
use crate::plot::commands::DECLARE_COMMANDS;
use crate::plot::{self, database, Plot, PLOT_BLOCK_HEIGHT};
use crate::utils::{self, HyphenatedUUID};
use backtrace::Backtrace;
use hmac::{Hmac, Mac};
use mchprs_network::packets::clientbound::{
    CConfigurationPluginMessage, CDisconnectLogin, CFinishConfiguration, CGameEvent,
    CGameEventType, CKnownPacks, CKnownPacksEntry, CLogin, CLoginPluginRequest, CLoginSuccess,
    CPlayerInfoActions, CPlayerInfoAddPlayer, CPlayerInfoUpdate, CPlayerInfoUpdatePlayer, CPong,
    CRegistryData, CRegistryDataEntry, CSetCompression, CSetContainerContent, CSetHeldItem,
    CStatusReponse, CSynchronizePlayerPosition, ClientBoundPacket, UpdateTime,
};
use mchprs_network::packets::serverbound::{
    SAcknowledgeFinishConfiguration, SHandshake, SLoginAcknowledged, SLoginPluginResponse,
    SLoginStart, SPing, SRequest, ServerBoundPacketHandler, VelocityResponseData,
};
use mchprs_network::packets::{PacketEncoderExt, PlayerProperty, SlotData, COMPRESSION_THRESHOLD};
use mchprs_network::{HandshakingConn, NetworkServer, NetworkState, PlayerPacketSender};
use mchprs_text::TextComponent;
use mchprs_utils::map;
use mchprs_world::{MC_VERSION, PROTOCOL_VERSION};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::Sha256;
use std::fs;
use std::io::{self, Cursor, Write as _};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

#[cfg(feature = "luckperms")]
use crate::permissions;

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
    pub properties: Vec<PlayerProperty>,
}

#[derive(Debug, Clone)]
struct PlayerListEntry {
    plot_x: i32,
    plot_z: i32,
    username: String,
    properties: Vec<PlayerProperty>,
    gamemode: Gamemode,
}

struct PlotListEntry {
    plot_x: i32,
    plot_z: i32,
    broadcast_sender: mpsc::Sender<BroadcastMessage>,
    priv_message_sender: mpsc::Sender<PrivMessage>,
}

#[derive(Clone, Serialize, Deserialize)]
struct WhitelistEntry {
    uuid: HyphenatedUUID,
    name: String,
}

/// This represents a minecraft server
pub struct MinecraftServer {
    network: NetworkServer,
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
            let thread = std::thread::current();
            let name = thread.name().unwrap_or("unnamed");
            error!("Thread '{name}' {panic_info}\n{backtrace:?}");
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
        let ctrl_handler_sender = plot_tx.clone();

        ctrlc::set_handler(move || {
            ctrl_handler_sender.send(Message::Shutdown).unwrap();
        })
        .expect("There was an error setting the ctrlc handler");

        let whitelist = CONFIG.whitelist.then(|| {
            let content = match fs::read("whitelist.json") {
                Ok(content) => content,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    fs::write("whitelist.json", b"[]").expect("Failed to create whitelist.json");
                    Vec::new()
                }
                Err(error) => panic!("Failed to read whitelist.json: {error}"),
            };
            if content.trim_ascii().is_empty() {
                Vec::new()
            } else {
                serde_json::from_slice(&content).expect("Invalid whitelist.json")
            }
        });

        #[cfg(feature = "luckperms")]
        if let Some(permissions_config) = &CONFIG.luckperms {
            permissions::init(permissions_config.clone()).unwrap();
        }

        // Create server struct
        let mut server = MinecraftServer {
            network: NetworkServer::new(bind_addr),
            receiver: server_rx,
            plot_sender: plot_tx,
            online_players: FxHashMap::default(),
            running_plots: Vec::new(),
            whitelist,
        };

        // Load the spawn area plot on server start
        // This plot should be always active
        let (broadcast_tx, broadcast_rx) = mpsc::channel();
        let (spawn_tx, spawn_rx) = mpsc::channel();
        Plot::load_and_run(
            0,
            0,
            broadcast_rx,
            server.plot_sender.clone(),
            spawn_rx,
            true,
            None,
        );
        server.running_plots.push(PlotListEntry {
            plot_x: 0,
            plot_z: 0,
            broadcast_sender: broadcast_tx,
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

    fn broadcast(&self, message: BroadcastMessage) {
        for plot in &self.running_plots {
            let _ = plot.broadcast_sender.send(message.clone());
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
        self.broadcast(BroadcastMessage::Shutdown);
        // Wait for all plots to save and unload
        while !self.running_plots.is_empty() {
            let Ok(message) = self.receiver.recv() else {
                break;
            };
            if let Message::PlotUnload(plot_x, plot_z) = message {
                self.handle_plot_unload(plot_x, plot_z);
            }
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
                properties: player.properties.clone(),
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
            let (broadcast_tx, broadcast_rx) = mpsc::channel();
            let (priv_tx, priv_rx) = mpsc::channel();
            Plot::load_and_run(
                plot_x,
                plot_z,
                broadcast_rx,
                self.plot_sender.clone(),
                priv_rx,
                false,
                Some(player),
            );
            self.running_plots.push(PlotListEntry {
                plot_x,
                plot_z,
                broadcast_sender: broadcast_tx,
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

        let uuid = client.uuid.unwrap();
        let username = client.username.clone().unwrap();
        let properties = client.properties.clone();

        #[cfg(feature = "luckperms")]
        let permissions_cache = match CONFIG
            .luckperms
            .is_some()
            .then(|| permissions::load_player_cache(uuid))
            .transpose()
        {
            Ok(permissions_cache) => permissions_cache,
            Err(err) => {
                use mchprs_network::packets::clientbound::CDisconnect;
                error!("Failed to load permissions for {username}: {err:?}");
                let disconnect = CDisconnect {
                    reason: "Could not load your permissions. Please try again later.".into(),
                }
                .encode();
                client.send_packet(&disconnect);
                client.close_connection();
                return;
            }
        };

        let load_player_info = LoadPlayerInfo {
            uuid,
            username,
            properties,
            #[cfg(feature = "luckperms")]
            permissions_cache,
            client: client.into(),
        };
        let player = Player::load_player(load_player_info);

        let join_game = CLogin {
            entity_id: player.entity_id as i32,
            is_hardcore: false,
            dimension_names: vec!["minecraft:overworld".to_owned()],
            max_players: CONFIG.max_players as i32,
            view_distance: CONFIG.view_distance as i32,
            simulation_distance: CONFIG.view_distance as i32,
            reduced_debug_info: false,
            enable_respawn_screen: false,
            do_limited_crafting: false,
            dimension_type: 0,
            dimension_name: "minecraft:overworld".to_owned(),
            hashed_seed: 0,
            gamemode: player.gamemode.get_id() as u8,
            previous_gamemode: 1,
            is_debug: false,
            is_flat: true,
            death_location: None,
            portal_cooldown: 0,
            sea_level: 16,
            enforces_secure_chat: false,
        }
        .encode();
        player.client.send_packet(&join_game);

        let entity_metadata = player.metadata_packet().encode();
        player.client.send_packet(&entity_metadata);

        // Send the player's position and rotation.
        let player_pos_and_look = CSynchronizePlayerPosition {
            teleport_id: 0,
            x: player.pos.x,
            y: player.pos.y,
            z: player.pos.z,
            velocity_x: 0.0,
            velocity_y: 0.0,
            velocity_z: 0.0,
            yaw: player.yaw,
            pitch: player.pitch,
            flags: 0,
        }
        .encode();
        player.client.send_packet(&player_pos_and_look);

        // Send the player list to the newly connected player.
        // (This is the list you see when you press tab in-game)
        let mut add_player_list = Vec::new();
        for (&uuid, player) in &self.online_players {
            add_player_list.push(CPlayerInfoUpdatePlayer {
                uuid,
                actions: CPlayerInfoActions {
                    add_player: Some(CPlayerInfoAddPlayer {
                        name: player.username.clone(),
                        properties: player.properties.clone(),
                    }),
                    update_gamemode: Some(player.gamemode.get_id()),
                    update_listed: Some(true),
                    ..Default::default()
                },
            });
        }
        add_player_list.push(CPlayerInfoUpdatePlayer {
            uuid: player.uuid,
            actions: CPlayerInfoActions {
                add_player: Some(CPlayerInfoAddPlayer {
                    name: player.username.clone(),
                    properties: player.properties.clone(),
                }),
                update_gamemode: Some(player.gamemode.get_id()),
                update_listed: Some(true),
                ..Default::default()
            },
        });

        let player_info = CPlayerInfoUpdate {
            players: add_player_list,
        }
        .encode();
        player.client.send_packet(&player_info);

        // Send the player's inventory
        let slot_data: Vec<Option<SlotData>> = player
            .inventory
            .iter()
            .map(|op| op.as_ref().map(utils::encode_slot_data))
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
            time_of_day: 6000,
            time_of_day_increasing: false,
        }
        .encode();
        player.client.send_packet(&time_update);

        player.update_player_abilities();

        let game_event = CGameEvent {
            reason: CGameEventType::WaitForChunks,
            value: 0.0,
        }
        .encode();
        player.client.send_packet(&game_event);

        self.plot_sender
            .send(Message::PlayerJoined(player))
            .unwrap();
    }

    fn handle_player_login_start(&mut self, client_idx: usize, login_start: SLoginStart) {
        let clients = &mut self.network.handshaking_clients;
        let username = login_start.name;
        clients[client_idx].username = Some(username.clone());

        if let Some(velocity_config) = &CONFIG.velocity
            && velocity_config.enabled
        {
            let message_id = rand::random();
            clients[client_idx].forwarding_message_id = Some(message_id);
            let plugin_message = CLoginPluginRequest {
                channel: "velocity:player_info".to_string(),
                message_id,
                data: vec![1], // MODERN_DEFAULT
            }
            .encode();
            clients[client_idx].send_packet(&plugin_message);
            return;
        }

        self.complete_player_login(client_idx);
    }

    fn complete_player_login(&mut self, client_idx: usize) {
        let clients = &mut self.network.handshaking_clients;
        let username = clients[client_idx].username.clone().unwrap();

        let set_compression = CSetCompression {
            threshold: COMPRESSION_THRESHOLD as i32,
        }
        .encode();
        clients[client_idx].send_packet(&set_compression);
        clients[client_idx].set_compressed(true);

        if let Some(whitelist) = &self.whitelist {
            // uuid will only be present if velocity is enabled in config
            let whitelisted = if let Some(uuid) = clients[client_idx].uuid {
                whitelist.iter().any(|entry| entry.uuid.0 == uuid)
            } else {
                whitelist.iter().any(|entry| entry.name == *username)
            };
            if !whitelisted {
                let client = clients.remove(client_idx);
                return refuse_login(&client, "You are not whitelisted on this server");
            }
        }

        if self.online_players.len() as i64 >= CONFIG.max_players {
            return refuse_login(&clients.remove(client_idx), "The server is full");
        }

        // Set generate uuid if it doesn't exist yet
        let uuid = match clients[client_idx].uuid {
            Some(uuid) => uuid,
            None => {
                let uuid = Player::generate_offline_uuid(&username);
                clients[client_idx].uuid = Some(uuid);
                uuid
            }
        };

        let login_success = CLoginSuccess {
            uuid,
            username,
            properties: clients[client_idx].properties.clone(),
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
                    properties: player.properties.clone(),
                };
                database::ensure_user(&format!("{:032x}", player.uuid), &player.username);
                self.broadcast(BroadcastMessage::PlayerJoinedInfo(player_join_info));
                self.send_player_to_plot(player, true);
            }
            Message::PlayerLeft(uuid) => {
                if let Some((_, player)) = self.online_players.remove_entry(&uuid) {
                    info!("{} left the game", player.username);
                }
                self.broadcast(BroadcastMessage::PlayerLeft(uuid));
            }
            Message::PlotUnload(plot_x, plot_z) => self.handle_plot_unload(plot_x, plot_z),
            Message::ChatInfo(uuid, username, message) => {
                info!("<{}> {}", username, message);
                self.broadcast(BroadcastMessage::Chat(
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
                if let Some((_, other_player)) = self
                    .online_players
                    .iter()
                    .find(|(_, p)| p.username.eq_ignore_ascii_case(&other_username))
                {
                    let plot_x = other_player.plot_x;
                    let plot_z = other_player.plot_z;
                    let other_username = other_player.username.clone();

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
                self.broadcast(BroadcastMessage::PlayerUpdateGamemode(uuid, gamemode));
            }
            Message::WhitelistAdd(uuid, username, sender) => {
                self.update_whitelist(sender, |whitelist| {
                    if whitelist.iter().any(|entry| entry.uuid.0 == uuid) {
                        return Err("That player is already whitelisted.");
                    }
                    let message = format!("{username} was successfully added to the whitelist.");
                    whitelist.push(WhitelistEntry {
                        name: username,
                        uuid: HyphenatedUUID(uuid),
                    });
                    Ok(message)
                })
            }
            Message::WhitelistRemove(uuid, sender) => self.update_whitelist(sender, |whitelist| {
                let Some(entry) = whitelist.iter().find(|entry| entry.uuid.0 == uuid) else {
                    return Err("That player is not whitelisted on this server.");
                };
                let message = format!(
                    "{} was successfully removed from the whitelist.",
                    entry.name
                );
                whitelist.retain(|entry| entry.uuid.0 != uuid);
                Ok(message)
            }),
        }
    }

    fn update_whitelist(
        &mut self,
        sender: PlayerPacketSender,
        change: impl FnOnce(&mut Vec<WhitelistEntry>) -> Result<String, &'static str>,
    ) {
        let Some(mut whitelist) = self.whitelist.clone() else {
            sender.send_error_message("Whitelist is not enabled!");
            return;
        };
        let message = match change(&mut whitelist) {
            Ok(message) => message,
            Err(message) => {
                sender.send_error_message(message);
                return;
            }
        };
        let result = serde_json::to_vec_pretty(&whitelist)
            .map_err(io::Error::from)
            .and_then(|data| {
                mchprs_save_data::replace_file("whitelist.json", |file| file.write_all(&data))
            });
        if let Err(error) = result {
            warn!("Failed to save whitelist: {error}");
            sender.send_error_message("Unable to save the whitelist. No changes were applied.");
            return;
        }
        self.whitelist = Some(whitelist);
        sender.send_system_message(&message);
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
                if self.network.handshaking_clients.len() != clients_len {
                    break;
                }
            }

            let new_len = self.network.handshaking_clients.len();

            if clients_len == new_len {
                client_idx += 1;
            }
            clients_len = new_len;
        }
    }
}

fn refuse_login(client: &HandshakingConn, reason: &str) {
    let disconnect = CDisconnectLogin {
        reason: json!({ "text": reason }).to_string(),
    }
    .encode();
    client.send_packet(&disconnect);
    client.close_connection();
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
        }
    }

    fn handle_request(&mut self, _request: SRequest, client_idk: usize) {
        let client = &mut self.network.handshaking_clients[client_idk];
        let response = CStatusReponse {
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
        self.handle_player_login_start(client_idx, login_start);
    }

    fn handle_login_acknowledged(
        &mut self,
        _login_acknowledged: SLoginAcknowledged,
        client_idx: usize,
    ) {
        let clients = &mut self.network.handshaking_clients;
        if clients[client_idx].uuid.is_none() {
            clients.remove(client_idx).close_connection();
            return;
        }
        // The client has been switched to the Configuration state
        let client = &mut clients[client_idx];

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

        let known_packs = [
            CKnownPacksEntry {
                namespace: "minecraft",
                id: "core",
                version: "1.21.8",
            },
            CKnownPacksEntry {
                namespace: "mchprs",
                id: "core",
                version: "0.0.0",
            },
        ];
        let known_packs = CKnownPacks {
            known_packs: &known_packs,
        }
        .encode();
        client.send_packet(&known_packs);

        let damage_types = [
            CRegistryDataEntry::new("minecraft:cactus"),
            CRegistryDataEntry::new("minecraft:campfire"),
            CRegistryDataEntry::new("minecraft:cramming"),
            CRegistryDataEntry::new("minecraft:dragon_breath"),
            CRegistryDataEntry::new("minecraft:drown"),
            CRegistryDataEntry::new("minecraft:dry_out"),
            CRegistryDataEntry::new("minecraft:ender_pearl"),
            CRegistryDataEntry::new("minecraft:fall"),
            CRegistryDataEntry::new("minecraft:fly_into_wall"),
            CRegistryDataEntry::new("minecraft:freeze"),
            CRegistryDataEntry::new("minecraft:generic"),
            CRegistryDataEntry::new("minecraft:generic_kill"),
            CRegistryDataEntry::new("minecraft:hot_floor"),
            CRegistryDataEntry::new("minecraft:in_fire"),
            CRegistryDataEntry::new("minecraft:in_wall"),
            CRegistryDataEntry::new("minecraft:lava"),
            CRegistryDataEntry::new("minecraft:lightning_bolt"),
            CRegistryDataEntry::new("minecraft:magic"),
            CRegistryDataEntry::new("minecraft:on_fire"),
            CRegistryDataEntry::new("minecraft:out_of_world"),
            CRegistryDataEntry::new("minecraft:outside_border"),
            CRegistryDataEntry::new("minecraft:stalagmite"),
            CRegistryDataEntry::new("minecraft:starve"),
            CRegistryDataEntry::new("minecraft:sweet_berry_bush"),
            CRegistryDataEntry::new("minecraft:wither"),
        ];
        let dimension_nbt = nbt::Blob::with_content(map! {
            "ambient_light" => nbt::Value::Float(1.0),
            "bed_works" => nbt::Value::Byte(1),
            "coordinate_scale" => nbt::Value::Double(1.0),
            "effects" => nbt::Value::String("minecraft:overworld".to_string()),
            "fixed_time" => nbt::Value::Long(6000),
            "has_ceiling" => nbt::Value::Byte(0),
            "has_raids" => nbt::Value::Byte(1),
            "has_skylight" => nbt::Value::Byte(1),
            "height" => nbt::Value::Int(PLOT_BLOCK_HEIGHT),
            "infiniburn" => nbt::Value::String("#minecraft:infiniburn_overworld".to_string()),
            "logical_height" => nbt::Value::Int(PLOT_BLOCK_HEIGHT),
            "min_y" => nbt::Value::Int(0),
            "monster_spawn_block_light_limit" => nbt::Value::Int(0),
            "monster_spawn_light_level" => nbt::Value::Compound(map !{
                "max_inclusive" => nbt::Value::Int(7),
                "min_inclusive" => nbt::Value::Int(0),
                "type" => nbt::Value::String("minecraft:uniform".to_string())
            }),
            "natural" => nbt::Value::Byte(1),
            "piglin_safe" => nbt::Value::Byte(0),
            "respawn_anchor_works" => nbt::Value::Byte(0),
            "ultrawarm" => nbt::Value::Byte(0)
        });
        let dimension_types = [CRegistryDataEntry::with_nbt(
            "minecraft:overworld",
            dimension_nbt.clone(),
        )];
        let biomes = [CRegistryDataEntry::new("minecraft:plains")];
        let painting_variants = [CRegistryDataEntry::new("minecraft:kebab")];
        let wolf_variants = [CRegistryDataEntry::new("minecraft:ashen")];
        let wolf_sound_variants = [CRegistryDataEntry::new("minecraft:sad")];
        let pig_variants = [CRegistryDataEntry::new("minecraft:warm")];
        let cat_variants = [CRegistryDataEntry::new("minecraft:white")];
        let registries = [
            CRegistryData::new("minecraft:damage_type", &damage_types),
            CRegistryData::new("minecraft:dimension_type", &dimension_types),
            CRegistryData::new("minecraft:worldgen/biome", &biomes),
            CRegistryData::new("minecraft:painting_variant", &painting_variants),
            CRegistryData::new("minecraft:wolf_variant", &wolf_variants),
            CRegistryData::new("minecraft:wolf_sound_variant", &wolf_sound_variants),
            CRegistryData::new("minecraft:pig_variant", &pig_variants),
            CRegistryData::new("minecraft:frog_variant", &pig_variants),
            CRegistryData::new("minecraft:cat_variant", &cat_variants),
            CRegistryData::new("minecraft:cow_variant", &pig_variants),
            CRegistryData::new("minecraft:chicken_variant", &pig_variants),
        ];
        for packet in registries {
            client.send_packet(&packet.encode());
        }

        client.send_packet(&CFinishConfiguration.encode());
    }

    fn handle_acknowledge_finish_configuration(
        &mut self,
        _ackowledge_finish_configuration: SAcknowledgeFinishConfiguration,
        client_idx: usize,
    ) {
        self.handle_player_enter_play(client_idx);
    }

    fn handle_login_plugin_response(&mut self, packet: SLoginPluginResponse, client_idx: usize) {
        let clients = &mut self.network.handshaking_clients;

        if Some(packet.message_id) != clients[client_idx].forwarding_message_id {
            error!(
                "Unknown login plugin response with message id: {}",
                packet.message_id
            );
            return;
        }

        if !packet.successful {
            error!("Velocity forwarding channel not understood by client");
            return;
        }

        let velocity_response = VelocityResponseData::decode(&mut Cursor::new(&packet.data));

        let velocity_response = match velocity_response {
            Ok(velocity_response) => velocity_response,
            Err(err) => {
                error!("Could not decode velocity reponse data: {:?}", err);
                return;
            }
        };

        let secret = CONFIG.velocity.as_ref().unwrap().secret.as_bytes();
        let mut mac = <Hmac<Sha256>>::new_from_slice(secret).unwrap();
        mac.update(&packet.data[32..]);
        if mac.verify_slice(&packet.data[..32]).is_err() {
            error!("Failed to verify velocity secret!");
            return;
        };

        clients[client_idx].uuid = Some(velocity_response.uuid);
        clients[client_idx].properties = velocity_response.properties;
        self.complete_player_login(client_idx);
    }
}

pub fn get_version_string() -> String {
    format!(
        "{} v{}\n{}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        env!("GIT_HASH")
    )
}
