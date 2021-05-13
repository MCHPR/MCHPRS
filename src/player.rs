use crate::blocks::{BlockDirection, BlockFacing, BlockPos};
use crate::chat::ChatComponent;
use crate::items::{Item, ItemStack};
use crate::network::packets::clientbound::*;
use crate::network::packets::SlotData;
use crate::network::NetworkClient;
use crate::plot::worldedit::{WorldEditClipboard, WorldEditUndo};
use byteorder::{BigEndian, ReadBytesExt};
use log::warn;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::cmp;
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
    Survival,
    Creative,
    Spectator,
}

impl Gamemode {
    pub fn get_id(self) -> u32 {
        match self {
            Gamemode::Survival => 0,
            Gamemode::Creative => 1,
            Gamemode::Spectator => 3,
        }
    }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum DamageSource {
    InFire,
    LightningBolt,
    OnFire,
    Lava,
    HotFloor,
    InWall,
    Cramming,
    Drown,
    Starve,
    Cactus,
    Fall,
    FlyIntoWall,
    OutOfWorld,
    Generic,
    Magic,
    Wither,
    Anvil,
    FallingBlock,
    DragonBreath,
    DryOut,
    SweetBerryBush,
}

impl DamageSource {
    pub fn bypass_armor(self) -> bool {
        match self {
            DamageSource::InFire
            | DamageSource::OnFire
            | DamageSource::InWall
            | DamageSource::Cramming
            | DamageSource::Drown
            | DamageSource::Starve
            | DamageSource::Fall
            | DamageSource::FlyIntoWall
            | DamageSource::OutOfWorld
            | DamageSource::Generic
            | DamageSource::Magic
            | DamageSource::Wither
            | DamageSource::DragonBreath => true,
            _ => false,
        }
    }

    pub fn is_fire(self) -> bool {
        match self {
            DamageSource::InFire
            | DamageSource::OnFire
            | DamageSource::Lava
            | DamageSource::HotFloor => true,
            _ => false,
        }
    }

    pub fn bypass_magic(self) -> bool {
        match self {
            DamageSource::Starve => true,
            _ => false,
        }
    }

    pub fn is_magic(self) -> bool {
        match self {
            DamageSource::Magic => true,
            _ => false,
        }
    }

    pub fn bypass_invulnerable(self) -> bool {
        match self {
            DamageSource::OutOfWorld => true,
            _ => false,
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
    health: f32,
    food: i8,
    saturation: f32,
    exhaustion: f32,
    food_timer: i8,
    fall_distance: f64,
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
    pub health: f32,
    pub food: i8,
    pub saturation: f32,
    pub exhaustion: f32,
    pub food_timer: i8,
    // When the player starts falling, this is the block they started falling at
    pub fall_distance: f64,
    pub invulnerable: bool,
    pub allow_flight: bool,

    pub eating: i8,
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
                health: player_data.health,
                food: player_data.food,
                saturation: player_data.saturation,
                exhaustion: player_data.exhaustion,
                food_timer: player_data.food_timer,
                fall_distance: player_data.fall_distance,
                invulnerable: false,
                allow_flight: true,
                eating: 0,
            }
        } else {
            Player::create_player(uuid, username, client)
        }
    }

    /// Returns the default player struct
    fn create_player(uuid: u128, username: String, client: NetworkClient) -> Player {
        let inventory: Vec<Option<ItemStack>> = vec![None; 46];
        Player {
            uuid,
            username,
            skin_parts: Default::default(),
            selected_slot: 0,
            x: 128f64,
            y: 9f64,
            z: 128f64,
            last_chunk_x: 8,
            last_chunk_z: 8,
            yaw: 0f32,
            pitch: 0f32,
            entity_id: client.id,
            client,
            inventory,
            flying: true,
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
            health: 20.0,
            food: 20,
            saturation: 5.0,
            exhaustion: 0.0,
            food_timer: 0,
            fall_distance: 0f64,
            invulnerable: false,
            allow_flight: true,
            eating: 0,
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
                })
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
            health: self.health,
            food: self.food,
            saturation: self.saturation,
            exhaustion: self.exhaustion,
            food_timer: self.food_timer,
            fall_distance: self.fall_distance,
        })
        .unwrap();
        file.write_all(&data).unwrap();
    }

    pub fn enable_food(&self) -> bool {
        match self.gamemode {
            Gamemode::Creative | Gamemode::Spectator => false,
            _ => false,
        }
    }

    pub fn tick(&mut self, tick_count: u64) {
        if self.y <= -64.0 && tick_count % 10 == 0 && self.health > 0.0 {
            self.hurt(DamageSource::OutOfWorld, 4.0);
        }

        if self.eating > 0 {
            self.eating -= 1;

            if self.eating == 0 {
                let mut stack_empty = false;
                let current_food = self.food;
                let current_saturation = self.saturation;
                let selected_slot = self.selected_slot as usize + 36;

                if let Some(item_stack) = &mut self.inventory[selected_slot] {
                    let food = item_stack.item_type.food();
                    if food > 0 {
                        let saturation = item_stack.item_type.saturation();

                        let new_food_level = current_food + food;
                        let new_saturation_level = if current_saturation + saturation > 5.0 {
                            5.0
                        } else {
                            current_saturation + saturation
                        };
                        item_stack.count -= 1;
                        stack_empty = item_stack.count == 0;

                        self.set_food(cmp::min(new_food_level, 20), new_saturation_level);
                        self.update_inventory_item(selected_slot);
                    }
                }
                if stack_empty {
                    self.inventory[selected_slot] = None;
                }
            }
        }

        if self.enable_food() {
            if self.food >= 20 && self.food_timer >= 10 && self.saturation > 0.0 {
                let heal_amount = if self.saturation >= 1.5 {
                    self.saturation -= 1.5;
                    2.0
                } else {
                    self.saturation = 0.0;
                    self.saturation * 2.0 + (2.0 / 3.0)
                };

                let new_health = if self.health + heal_amount < 20.0 {
                    self.health + heal_amount
                } else {
                    20.0
                };

                self.set_health(new_health);
                self.food_timer = 0;
            } else if self.food > 17 && self.food_timer >= 80 {
                self.health += 1.0;
                self.exhaustion += 6.0;
                self.send_health();
            } else if self.food == 0 && self.food_timer >= 80 {
                if self.health > 10.0 {
                    let take = if self.health < 11.0 {
                        self.health - 10.0
                    } else {
                        1.0
                    };
                    self.hurt(DamageSource::Starve, take);
                }
            }
            if self.food_timer >= 80 {
                self.food_timer = 0;
            }

            if self.food > 17 || self.food == 0 {
                self.food_timer += 1;
            }

            if self.exhaustion > 4.0 {
                if self.saturation > 0.0 {
                    self.saturation -= if self.saturation >= 1.0 {
                        1.0
                    } else {
                        self.saturation
                    };
                } else {
                    if self.food > 0 {
                        self.food -= 1;
                    }
                }
                self.exhaustion = 0.0;
            }
        }
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

    pub fn send_health(&mut self) {
        let update_health = C49UpdateHealth {
            health: self.health,
            food: self.food as i32,
            saturation: self.saturation,
        }
        .encode();
        self.client.send_packet(&update_health);

        if self.health <= 0.0 {
            let death_screen = C31CombatEvent {
                player_id: self.entity_id as i32,
                entity_id: -1,
                message: json!({ "text": "You died!"}).to_string(),
            }
            .encode();
            self.client.send_packet(&death_screen);

            // let death_status = C1AEntityStatus {
            //     entity_id: self.entity_id as i32,
            //     status: C1AEntityStatuses::Death,
            // };
            // for other_player in 0..self.plot.players.len() {
            //   other_player.client.send_packet(&death_status);
            // }
        }
    }

    pub fn respawn(&mut self) {
        let respawn = C39Respawn {
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
            world_name: "mchprs:world".to_string(),
            hashed_seed: 0,
            gamemode: self.gamemode.get_id() as u8,
            previous_gamemode: self.gamemode.get_id() as u8,
            is_debug: false,
            is_flat: true,
            copy_metadata: true,
        }
        .encode();
        self.client.send_packet(&respawn);

        self.teleport(128f64, 9f64, 128f64);
        self.health = 20.0;
        self.food = 20;
        self.saturation = 5.0;
        self.send_health()
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

    /// Sends the ChatMessage packet containing the raw json data.
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

    /// Sends the ChatMessage packet containing the raw json data.
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

    /// Sends a regular chat message to the player (`message` is not in json format)
    pub fn send_chat_message(&mut self, sender: u128, message: Vec<ChatComponent>) {
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
        let player_abilities = C30PlayerAbilities {
            flags: (self.invulnerable as u8)
                | ((self.flying as u8) << 1)
                | ((self.allow_flight as u8) << 2)
                | ((matches!(self.gamemode, Gamemode::Creative) as u8) << 3),
            fly_speed: 0.05 * self.fly_speed,
            fov_modifier: 0.1,
        }
        .encode();
        self.client.send_packet(&player_abilities);
    }

    pub fn set_gamemode(&mut self, gamemode: Gamemode) {
        self.fall_distance = 0.0;
        self.gamemode = gamemode;
        let change_game_state = CChangeGameState {
            reason: CChangeGameStateReason::ChangeGamemode,
            value: self.gamemode.get_id() as f32,
        }
        .encode();
        self.client.send_packet(&change_game_state);
        self.update_player_abilities();
    }

    pub fn set_health(&mut self, health: f32) {
        self.health = health;

        self.send_health()
    }

    pub fn hurt(&mut self, source: DamageSource, amount: f32) {
        if (self.invulnerable || matches!(self.gamemode, Gamemode::Creative | Gamemode::Spectator))
            && !source.bypass_invulnerable()
        {
            return;
        }

        self.health -= amount;

        if !source.bypass_armor() && self.enable_food() {
            self.exhaustion += 0.1;
        }

        let entity_animation_damage = C05EntityAnimation {
            entity_id: self.entity_id as i32,
            animation: 1,
        }
        .encode();

        self.client.send_packet(&entity_animation_damage);

        self.send_health();
    }

    pub fn set_food(&mut self, food: i8, saturation: f32) {
        self.food = food;
        self.saturation = saturation;

        if self.saturation > food as f32 {
            self.saturation = food as f32;
        }

        self.send_health();
    }

    pub fn update_inventory_item(&mut self, slot: usize) {
        let set_slot = C15SetSlot {
            window: if slot >= 36 && slot <= 45 { 0 } else { -2 },
            slot: slot as i16,
            slot_data: self.inventory[slot].as_ref().map(|item| SlotData {
                item_count: item.count as i8,
                item_id: item.item_type.get_id() as i32,
                nbt: item.nbt.clone(),
            }),
        }
        .encode();
        self.client.send_packet(&set_slot);
    }

    pub fn set_inventory_item(&mut self, slot: usize, item: Option<ItemStack>) {
        self.inventory[slot] = item.clone();
        self.update_inventory_item(slot);
    }
}
