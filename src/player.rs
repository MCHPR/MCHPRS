
use crate::blocks::{BlockDirection, BlockPos};
use crate::items::{Item, ItemStack};
use crate::network::packets::clientbound::*;
use crate::network::NetworkClient;
use crate::plot::worldedit::WorldEditClipboard;

use byteorder::{BigEndian, ReadBytesExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{Cursor, Write};
use std::time::{Instant, SystemTime};

#[derive(Debug, Serialize, Deserialize)]
pub struct InventoryEntry {
    id: u32,
    slot: i8,
    count: i8,
    damage: i16,
    nbt: Option<Vec<u8>>,
}

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
    pub selected_slot: u32,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub last_chunk_x: i32,
    pub last_chunk_z: i32,
    pub yaw: f32,
    pub pitch: f32,
    pub flying: bool,
    pub sprinting: bool,
    pub crouching: bool,
    pub on_ground: bool,
    pub fly_speed: f32,
    pub walk_speed: f32,
    pub entity_id: u32,

    // Networking
    pub client: NetworkClient,
    pub last_keep_alive_received: Instant,
    last_keep_alive_sent: Instant,

    // Worldedit
    pub first_position: Option<BlockPos>,
    pub second_position: Option<BlockPos>,
    pub worldedit_clipboard: Option<WorldEditClipboard>,
}

impl fmt::Debug for Player {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Player")
            .field("username", &self.username)
            .field("uuid", &Player::uuid_with_hyphens(self.uuid))
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

    pub fn uuid_with_hyphens(uuid: u128) -> String {
        let mut hex = format!("{:032x}", uuid);
        hex.insert(8, '-');
        hex.insert(13, '-');
        hex.insert(18, '-');
        hex.insert(23, '-');
        hex
    }

    pub fn load_player(uuid: u128, username: String, client: NetworkClient) -> Player {
        if let Ok(data) = fs::read(format!("./world/players/{:032x}", uuid)) {
            // TODO: Handle format error
            let player_data: PlayerData = bincode::deserialize(&data).unwrap();

            let mut inventory: Vec<Option<ItemStack>> = vec![];
            inventory.resize_with(46, || None);
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
                on_ground: player_data.on_ground,
                walk_speed: player_data.walk_speed,
                fly_speed: player_data.fly_speed,
                last_keep_alive_received: Instant::now(),
                last_keep_alive_sent: Instant::now(),
                first_position: None,
                second_position: None,
                worldedit_clipboard: None,
            }
        } else {
            Player::create_player(uuid, username, client)
        }
    }

    fn create_player(uuid: u128, username: String, client: NetworkClient) -> Player {
        let mut inventory: Vec<Option<ItemStack>> = vec![];
        inventory.resize_with(46, || None);

        Player {
            uuid,
            username,
            skin_parts: Default::default(),
            selected_slot: 0,
            x: 64f64,
            y: 64f64,
            z: 64f64,
            last_chunk_x: 4,
            last_chunk_z: 4,
            yaw: 0f32,
            pitch: 0f32,
            entity_id: client.id,
            client,
            inventory,
            flying: false,
            sprinting: false,
            crouching: false,
            fly_speed: 1f32,
            walk_speed: 1f32,
            on_ground: true,
            last_keep_alive_received: Instant::now(),
            last_keep_alive_sent: Instant::now(),
            first_position: None,
            second_position: None,
            worldedit_clipboard: None,
        }
    }

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
                })
            }
        }

        let data = bincode::serialize(&PlayerData {
            fly_speed: self.fly_speed,
            flying: self.flying,
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

    pub fn update_view_pos(&mut self) {
        let chunk_x = self.x as i32 >> 4;
        let chunk_z = self.z as i32 >> 4;
        
        if chunk_x != self.last_chunk_x || chunk_z != self.last_chunk_z {
            let update_view = C41UpdateViewPosition { chunk_x, chunk_z }.encode();
            self.client.send_packet(&update_view);
        }

        self.last_chunk_x = chunk_x;
        self.last_chunk_z = chunk_z;
    }

    pub fn update(&mut self) {
        if self.last_keep_alive_received.elapsed().as_secs() > 30 {
            self.kick("Timed out".to_string());
        }

        if self.last_keep_alive_sent.elapsed().as_secs() > 10 {
            self.send_keep_alive();
        }

        self.update_view_pos();
        if let Err(err) = self.client.update() {
            self.kick(
                json!({ "text": format!("There was an error reading a packet header: {:?}", err) })
                    .to_string(),
            );
        }
    }

    pub fn send_keep_alive(&mut self) {
        let keep_alive = C21KeepAlive {
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

    pub fn teleport(&mut self, x: f64, y: f64, z: f64) {
        let player_position_and_look = C36PlayerPositionAndLook {
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
        self.update_view_pos();
    }

    pub fn send_raw_chat(&mut self, message: String) {
        let chat_message = C0FChatMessage {
            message,
            position: 0,
        }

        .encode();
        self.client.send_packet(&chat_message);
    }

    pub fn send_raw_system_message(&mut self, message: String) {
        let chat_message = C0FChatMessage {
            message,
            position: 1,
        }
        
        .encode();
        self.client.send_packet(&chat_message);
    }

    pub fn send_chat_message(&mut self, message: String) {
        self.send_raw_chat(json!({ "text": message }).to_string());
    }

    pub fn send_system_message(&mut self, message: &str) {
        self.send_raw_system_message(
            json!({
                "text": message,
                "color": "yellow"
            })
            .to_string(),
        );
    }

    pub fn send_error_message(&mut self, message: &str) {
        self.send_raw_system_message(
            json!({
                "text": message,
                "color": "red"
            })
            .to_string(),
        );
    }

    pub fn send_worldedit_message(&mut self, message: &str) {
        self.send_raw_system_message(
            json!({
                "text": message,
                "color": "light_purple"
            })
            .to_string(),
        );
    }

    pub fn worldedit_set_first_position(&mut self, x: i32, y: u32, z: i32) {
        self.send_worldedit_message(&format!("First position set to ({}, {}, {})", x, y, z));
        self.first_position = Some(BlockPos::new(x, y, z));
    }

    pub fn worldedit_set_second_position(&mut self, x: i32, y: u32, z: i32) {
        self.send_worldedit_message(&format!("Second position set to ({}, {}, {})", x, y, z));
        self.second_position = Some(BlockPos::new(x, y, z));
    }

    pub fn kick(&mut self, reason: String) {
        let disconnect = C1BDisconnect { reason }.encode();
        self.client.send_packet(&disconnect);
    }
}
