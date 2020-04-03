use crate::network::NetworkClient;
use byteorder::{LittleEndian, ReadBytesExt};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{self, File};
use std::io::Cursor;
use std::sync::{Arc, RwLock};

#[derive(Debug, Serialize, Deserialize)]
pub struct InventoryEntry {
    id: String,
    slot: i8,
    count: i8,
    damage: i16,
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
        const Cape = 0x01;
        const Jacket = 0x02;
        const LeftSleeve = 0x04;
        const RightSleeve = 0x08;
        const LeftPantsLeg = 0x10;
        const RightPantsLeg = 0x20;
        const Hat = 0x40;
    }
}

#[derive(Clone)]
pub struct Item {
    id: String,
    count: u8,
    damage: u16,
}

pub struct Player {
    pub uuid: u128,
    pub username: String,
    pub skin_parts: SkinParts,
    pub inventory: Vec<Option<Item>>,
    pub selected_slot: u32,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub client: NetworkClient,
    pub flying: bool,
    pub on_ground: bool,
    pub fly_speed: f32,
    pub walk_speed: f32,
}

impl fmt::Debug for Player {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Player")
            .field("username", &self.username)
            .field("uuid", &format!("{:032x}", self.uuid))
            .finish()
    }
}

impl Player {
    pub fn generate_offline_uuid(username: &str) -> u128 {
        Cursor::new(md5::compute(username).0)
            .read_u128::<LittleEndian>()
            .unwrap()
    }

    pub fn load_player(uuid: u128, username: String, client: NetworkClient) -> Player {
        if let Ok(data) = fs::read(format!("./world/players/{:032X}", uuid)) {
            // TODO: Handle format error
            let player_data: PlayerData = nbt::from_reader(Cursor::new(data)).unwrap();

            let mut inventory: Vec<Option<Item>> = vec![];
            inventory.resize_with(46, || None);
            for entry in player_data.inventory {
                inventory[entry.slot as usize] = Some(Item {
                    id: entry.id,
                    count: entry.count as u8,
                    damage: entry.damage as u16,
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
                client,
                flying: player_data.flying,
                on_ground: player_data.on_ground,
                walk_speed: player_data.walk_speed,
                fly_speed: player_data.fly_speed,
            }
        } else {
            Player::create_player(uuid, username, client)
        }
    }

    fn create_player(uuid: u128, username: String, client: NetworkClient) -> Player {
        let mut inventory: Vec<Option<Item>> = vec![];
        inventory.resize_with(46, || None);
        Player {
            uuid,
            username,
            skin_parts: Default::default(),
            selected_slot: 0,
            x: 64f64,
            y: 64f64,
            z: 64f64,
            client,
            inventory,
            fly_speed: 1f32,
            walk_speed: 1f32,
            on_ground: true,
            flying: false,
        }
    }

    fn save(&self) {
        let mut file = File::open(format!("./world/players/{:032X}", self.uuid)).unwrap();
        let mut inventory: Vec<InventoryEntry> = Vec::new();
        for (slot, item_option) in self.inventory.iter().enumerate() {
            if let Some(item) = item_option {
                inventory.push(InventoryEntry {
                    count: item.count as i8,
                    id: item.id.clone(),
                    damage: item.damage as i16,
                    slot: slot as i8,
                })
            }
        }
        nbt::to_writer(
            &mut file,
            &PlayerData {
                fly_speed: self.fly_speed,
                flying: self.flying,
                inventory,
                motion: vec![0f64, 0f64, 0f64],
                on_ground: self.on_ground,
                position: vec![self.x, self.y, self.z],
                rotation: vec![0f32, 0f32, 0f32],
                selected_item_slot: self.selected_slot as i32,
                walk_speed: self.walk_speed,
            },
            None,
        )
        .unwrap();
    }

    pub fn teleport(&mut self, x: f64, y: f64, z: f64) {}
}
