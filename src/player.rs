use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Cursor;
use std::sync::{Arc, RwLock};

#[derive(Debug, Serialize, Deserialize)]
pub struct InventoryEntry {
    id: String,
    #[serde(rename = "Slot")]
    slot: Option<i8>,
    #[serde(rename = "Count")]
    count: i8,
    #[serde(rename = "Damage")]
    damage: i16,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InventoryEntryDisplay {
    #[serde(rename = "Name")]
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlayerAbilityData {
    invulnerable: bool,
    instabuild: bool,
    flying: bool,
    #[serde(rename = "flySpeed")]
    fly_speed: f32,
    #[serde(rename = "walkSpeed")]
    walk_speed: f32,
    #[serde(rename = "mayBuild")]
    may_build: bool,
    #[serde(rename = "mayfly")]
    may_fly: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlayerData {
    abilities: PlayerAbilityData,

    #[serde(rename = "OnGround")]
    on_ground: bool,
    #[serde(rename = "Motion")]
    motion: Vec<f64>, // [f64; 3]
    #[serde(rename = "Pos")]
    position: Vec<f64>, // [f64; 3]
    #[serde(rename = "Rotation")]
    rotation: Vec<f32>, // [f32; 2]

    #[serde(rename = "PortalCooldown")]
    portal_cooldown: Option<i32>,
    #[serde(rename = "Invulnerable")]
    invulnerable: Option<bool>,

    #[serde(rename = "Inventory")]
    inventory: Vec<InventoryEntry>,

    #[serde(rename = "SelectedItemSlot")]
    selected_item_slot: Option<i32>,
}

bitflags! {
    #[derive(Default)]
    struct SkinParts: u32 {
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
struct Item {
    id: String,
}

pub struct Player {
    uuid: u128,
    username: String,
    skin_parts: SkinParts,
    inventory: Vec<Option<Item>>,
    selected_slot: u32,
    x: f64,
    y: f64,
    z: f64,
}

impl Player {
    fn generate_offline_uuid(username: String) {}

    /// This function returns `None` when a player is not found.
    fn load_player(uuid: u128, username: String) -> Option<Player> {
        if let Ok(data) = fs::read(format!("./world/players/{:032X}", uuid)) {
            // TODO: Handle format error
            let player_data: PlayerData = nbt::from_reader(Cursor::new(data)).unwrap();

            let mut inventory: Vec<Option<Item>> = vec![];
            inventory.resize_with(46, || None);
            for entry in player_data.inventory {
                if let Some(slot) = entry.slot {
                    inventory[slot as usize] = Some(Item { id: entry.id });
                }
            }
            Some(Player {
                uuid,
                username,
                skin_parts: Default::default(),
                inventory,
                selected_slot: player_data.selected_item_slot.unwrap_or(0) as u32,
                x: player_data.position[0],
                y: player_data.position[1],
                z: player_data.position[2],
            })
        } else {
            None
        }
    }

    fn create_player() {}
}
