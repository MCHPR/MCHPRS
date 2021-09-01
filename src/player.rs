use crate::blocks::{BlockDirection, BlockFacing, BlockPos};
use crate::chat::ChatComponent;
use crate::config::CONFIG;
use crate::items::{Item, ItemStack};
use crate::network::packets::clientbound::*;
use crate::network::NetworkClient;
use crate::permissions::{self, PlayerPermissionsCache};
use crate::plot::worldedit::{WorldEditClipboard, WorldEditUndo};
use crate::utils::HyphenatedUUID;
use byteorder::{BigEndian, ReadBytesExt};
use log::warn;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{Cursor, Write};
use std::time::{Instant, SystemTime};

/// This is a single item in the player's inventory
#[derive(Debug, Serialize, Deserialize)]
pub struct InventoryEntry {
    id: u32,
    slot: i8,
    count: i8,
    damage: i16,
    nbt: Option<Vec<u8>>,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum Gamemode {
    Creative,
    Spectator,
}

impl Gamemode {
    pub fn get_id(self) -> u32 {
        match self {
            Gamemode::Creative => 1,
            Gamemode::Spectator => 3,
        }
    }
}

/// This structure represents how the player will be
/// serialized when saved to it's file.
#[derive(Debug, Serialize, Deserialize)]
pub struct PlayerData {
    on_ground: bool,
    flying: bool,
    motion: Vec<f64>,   // [f64; 3]
    position: Vec<f64>, // [f64; 3]
    rotation: Vec<f32>, // [f32; 2]
    inventory: Vec<InventoryEntry>,
    selected_item_slot: i32,
    fly_speed: f32,
    walk_speed: f32,
    gamemode: Gamemode,
}

bitflags! {
    #[derive(Default)]
    pub struct SkinParts: u32 {
        const CAPE = 0x01;
        const JACKET = 0x02;
        const LEFT_SLEEVE = 0x04;
        const RIGHT_SLEEVE = 0x08;
        const LEFT_PANTS_LEG = 0x10;
        const RIGHT_PANTS_LEG = 0x20;
        const HAT = 0x40;
    }
}

pub struct Player {
    pub uuid: u128,
    pub username: String,
    pub skin_parts: SkinParts,
    pub inventory: Vec<Option<ItemStack>>,
    /// The selected slot of the player's hotbar (1-9)
    pub selected_slot: u32,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// The last X chunk the player was in. This is used for updated view position.
    pub last_chunk_x: i32,
    /// The last Z chunk the player was in. This is used for updated view position.
    pub last_chunk_z: i32,
    /// The player's head yaw rotation.
    pub yaw: f32,
    /// The player's head pitch rotation.
    pub pitch: f32,
    pub flying: bool,
    pub sprinting: bool,
    pub crouching: bool,
    pub on_ground: bool,
    pub fly_speed: f32,
    pub walk_speed: f32,
    pub gamemode: Gamemode,
    pub entity_id: u32,
    /// Packets are sent through the client.
    pub client: NetworkClient,
    /// The last time the keep alive packet was received.
    pub last_keep_alive_received: Instant,
    /// The last time the keep alive packet was sent.
    last_keep_alive_sent: Instant,
    /// The worldedit first position.
    pub first_position: Option<BlockPos>,
    /// The worldedit second position.
    pub second_position: Option<BlockPos>,
    /// The worldedit current clipboard.
    pub worldedit_clipboard: Option<WorldEditClipboard>,
    /// The saved sections used for worldedit //undo
    /// Each entry stores the plot coords and the clipboard
    pub worldedit_undo: Vec<WorldEditUndo>,
    /// Commands are stored so they can be handled after packets
    pub command_queue: Vec<String>,
    permissions_cache: Option<PlayerPermissionsCache>,
}

impl fmt::Debug for Player {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Player")
            .field("username", &self.username)
            .field("uuid", &HyphenatedUUID(self.uuid).to_string())
            .finish()
    }
}

impl Player {
    pub fn generate_offline_uuid(username: &str) -> u128 {
        Cursor::new(md5::compute(format!("OfflinePlayer:{}", username)).0)
            .read_u128::<BigEndian>()
            .unwrap()
            // Encode version and varient into uuid
            & (!(0xC << 60) & !(0xF << 76))
            | ((0x8 << 60) | (0x3 << 76))
    }

    /// This will load the player from the file. If the file does not exist,
    /// It will be created.
    pub fn load_player(uuid: u128, username: String, client: NetworkClient) -> Player {
        if let Ok(data) = fs::read(format!("./world/players/{:032x}", uuid)) {
            let player_data: PlayerData = match bincode::deserialize(&data) {
                Ok(data) => data,
                Err(_) => {
                    warn!("There was an error loading the player data for {}, player data will be reset.", username);
                    return Player::create_player(uuid, username, client);
                }
            };

            // Load inventory
            let mut inventory: Vec<Option<ItemStack>> = vec![None; 46];
            for entry in player_data.inventory {
                let nbt = entry
                    .nbt
                    .map(|data| nbt::Blob::from_reader(&mut Cursor::new(data)).unwrap());
                inventory[entry.slot as usize] = Some(ItemStack {
                    item_type: Item::from_id(entry.id),
                    count: entry.count as u8,
                    damage: entry.damage as u16,
                    nbt,
                });
            }
            let permissions_cache = CONFIG
                .luckperms
                .is_some()
                .then(|| permissions::load_player_cache(uuid).unwrap());
            Player {
                uuid,
                username,
                skin_parts: Default::default(),
                inventory,
                selected_slot: player_data.selected_item_slot as u32,
                x: player_data.position[0],
                y: player_data.position[1],
                z: player_data.position[2],
                pitch: player_data.rotation[0],
                yaw: player_data.rotation[1],
                last_chunk_x: 0,
                last_chunk_z: 0,
                entity_id: client.id,
                client,
                flying: player_data.flying,
                sprinting: false,
                crouching: false,
                gamemode: player_data.gamemode,
                on_ground: player_data.on_ground,
                walk_speed: player_data.walk_speed,
                fly_speed: player_data.fly_speed,
                last_keep_alive_received: Instant::now(),
                last_keep_alive_sent: Instant::now(),
                first_position: None,
                second_position: None,
                worldedit_clipboard: None,
                worldedit_undo: Vec::new(),
                command_queue: Vec::new(),
                permissions_cache,
            }
        } else {
            Player::create_player(uuid, username, client)
        }
    }

    /// Returns the default player struct
    fn create_player(uuid: u128, username: String, client: NetworkClient) -> Player {
        let inventory: Vec<Option<ItemStack>> = vec![None; 46];
        let permissions_cache = CONFIG
            .luckperms
            .is_some()
            .then(|| permissions::load_player_cache(uuid).unwrap());
        Player {
            uuid,
            username,
            skin_parts: Default::default(),
            selected_slot: 0,
            x: 128f64,
            y: 128f64,
            z: 128f64,
            last_chunk_x: 8,
            last_chunk_z: 8,
            yaw: 0f32,
            pitch: 0f32,
            entity_id: client.id,
            client,
            inventory,
            flying: false,
            sprinting: false,
            crouching: false,
            gamemode: Gamemode::Creative,
            fly_speed: 1f32,
            walk_speed: 1f32,
            on_ground: true,
            last_keep_alive_received: Instant::now(),
            last_keep_alive_sent: Instant::now(),
            first_position: None,
            second_position: None,
            worldedit_clipboard: None,
            worldedit_undo: Vec::new(),
            command_queue: Vec::new(),
            permissions_cache,
        }
    }

    /// Saves the player to `./world/players/{uuid}`. This will create
    /// the file if it does not already exist.
    pub fn save(&self) {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(format!("./world/players/{:032x}", self.uuid))
            .unwrap();
        let mut inventory: Vec<InventoryEntry> = Vec::new();
        for (slot, item_option) in self.inventory.iter().enumerate() {
            if let Some(item) = item_option {
                let nbt = item.nbt.clone().map(|blob| {
                    let mut data = Vec::new();
                    blob.to_writer(&mut data).unwrap();
                    data
                });
                inventory.push(InventoryEntry {
                    count: item.count as i8,
                    id: item.item_type.get_id(),
                    damage: item.damage as i16,
                    slot: slot as i8,
                    nbt,
                });
            }
        }
        let data = bincode::serialize(&PlayerData {
            fly_speed: self.fly_speed,
            flying: self.flying,
            gamemode: self.gamemode,
            inventory,
            motion: vec![0f64, 0f64, 0f64],
            on_ground: self.on_ground,
            position: vec![self.x, self.y, self.z],
            rotation: vec![self.pitch, self.yaw],
            selected_item_slot: self.selected_slot as i32,
            walk_speed: self.walk_speed,
        })
        .unwrap();
        file.write_all(&data).unwrap();
    }

    /// Manages keep alives and packet reading. Return true if the view position should be updated.
    pub fn update(&mut self) -> bool {
        if self.last_keep_alive_received.elapsed().as_secs() > 30 {
            self.kick(json!({ "text": "Timed out." }).to_string());
        }
        if self.last_keep_alive_sent.elapsed().as_secs() > 10 {
            self.send_keep_alive();
        }
        self.x as i32 >> 4 != self.last_chunk_x || self.z as i32 >> 4 != self.last_chunk_z
    }

    /// Sends the keep alive packet to the client and updates `last_keep_alive_sent`
    pub fn send_keep_alive(&mut self) {
        let keep_alive = CKeepAlive {
            id: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
        }
        .encode();
        self.client.send_packet(&keep_alive);
        self.last_keep_alive_sent = Instant::now();
    }

    pub fn get_direction(&self) -> BlockDirection {
        match ((self.yaw / 90.0 + 0.5).floor() as i32 & 3).abs() as u32 {
            0 => BlockDirection::South,
            1 => BlockDirection::West,
            2 => BlockDirection::North,
            3 => BlockDirection::East,
            _ => BlockDirection::South,
        }
    }

    pub fn get_facing(&self) -> BlockFacing {
        let yaw = self.yaw.rem_euclid(360.0);
        let pitch = self.pitch;
        if pitch <= -70.0 {
            BlockFacing::Up
        } else if pitch >= 70.0 {
            BlockFacing::Down
        } else if (45.0..=135.0).contains(&yaw) {
            BlockFacing::West
        } else if (135.0..=225.0).contains(&yaw) {
            BlockFacing::North
        } else if (225.0..=315.0).contains(&yaw) {
            BlockFacing::East
        } else {
            BlockFacing::South
        }
    }

    pub fn teleport(&mut self, x: f64, y: f64, z: f64) {
        let player_position_and_look = CPlayerPositionAndLook {
            x,
            y,
            z,
            yaw: 0f32,
            pitch: 0f32,
            flags: 0x08 | 0x10, // pitch and yaw are relative
            teleport_id: 0,
        }
        .encode();
        self.x = x;
        self.y = y;
        self.z = z;
        self.client.send_packet(&player_position_and_look);
    }

    /// Sends the `ChatMessage` packet containing the raw json data.
    /// Position 0: chat (chat box)
    pub fn send_raw_chat(&mut self, sender: u128, message: String) {
        let chat_message = CChatMessage {
            message,
            sender,
            position: 0,
        }
        .encode();
        self.client.send_packet(&chat_message);
    }

    /// Sends the `ChatMessage` packet containing the raw json data.
    /// Position 1: system message (chat box)
    pub fn send_raw_system_message(&mut self, message: String) {
        let chat_message = CChatMessage {
            message,
            sender: 0,
            position: 1,
        }
        .encode();
        self.client.send_packet(&chat_message);
    }

    /// Sends a raw chat message to the player
    pub fn send_chat_message(&mut self, sender: u128, message: &[ChatComponent]) {
        let json = json!({ "text": "", "extra": message }).to_string();
        self.send_raw_chat(sender, json);
    }

    /// Sends the player a yellow system message (`message` is not in json format)
    pub fn send_system_message(&mut self, message: &str) {
        self.send_raw_system_message(
            json!({
                "text": message,
                "color": "yellow"
            })
            .to_string(),
        );
    }

    pub fn send_no_permission_message(&mut self) {
        self.send_error_message("You do not have permission to perform this action.");
    }

    /// Sends the player a red system message (`message` is not in json format)
    pub fn send_error_message(&mut self, message: &str) {
        self.send_raw_system_message(
            json!({
                "text": message,
                "color": "red"
            })
            .to_string(),
        );
    }

    /// Sends the player a light purple system message (`message` is not in json format)
    pub fn send_worldedit_message(&mut self, message: &str) {
        self.send_raw_system_message(
            json!({
                "text": message,
                "color": "light_purple"
            })
            .to_string(),
        );
    }

    pub fn worldedit_set_first_position(&mut self, pos: BlockPos) {
        self.send_worldedit_message(&format!(
            "First position set to ({}, {}, {})",
            pos.x, pos.y, pos.z
        ));
        self.first_position = Some(pos);
        self.worldedit_send_cui(&format!("p|0|{}|{}|{}|0", pos.x, pos.y, pos.z));
    }

    pub fn worldedit_set_second_position(&mut self, pos: BlockPos) {
        self.send_worldedit_message(&format!(
            "Second position set to ({}, {}, {})",
            pos.x, pos.y, pos.z
        ));
        self.second_position = Some(pos);
        self.worldedit_send_cui(&format!("p|1|{}|{}|{}|0", pos.x, pos.y, pos.z));
    }

    pub fn worldedit_send_cui(&mut self, message: &str) {
        let cui_plugin_message = CPluginMessage {
            channel: String::from("worldedit:cui"),
            data: Vec::from(message.as_bytes()),
        }
        .encode();
        self.client.send_packet(&cui_plugin_message);
    }

    /// Sends the player the disconnect packet, it is still up to the player to end the network stream.
    pub fn kick(&mut self, reason: String) {
        let disconnect = CDisconnect { reason }.encode();
        self.client.send_packet(&disconnect);
    }

    pub fn update_player_abilities(&mut self) {
        let player_abilities = CPlayerAbilities {
            flags: 0x0D | ((self.flying as u8) << 1),
            fly_speed: 0.05 * self.fly_speed,
            fov_modifier: 0.1,
        }
        .encode();
        self.client.send_packet(&player_abilities);
    }

    pub fn set_gamemode(&mut self, gamemode: Gamemode) {
        self.gamemode = gamemode;
        let change_game_state = CChangeGameState {
            reason: CChangeGameStateReason::ChangeGamemode,
            value: self.gamemode.get_id() as f32,
        }
        .encode();
        self.client.send_packet(&change_game_state);
    }

    pub fn has_permission(&self, node: &str) -> bool {
        if let Some(cache) = &self.permissions_cache {
            if let Some(val) = cache.get_node_val(node) {
                val > 0
            } else {
                // Node is not in database
                false
            }
        } else {
            // Permissions is not enabled
            true
        }
    }
}
