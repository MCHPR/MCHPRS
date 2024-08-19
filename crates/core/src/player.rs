use crate::config::CONFIG;
use crate::permissions::{self, PlayerPermissionsCache};
use crate::plot::worldedit::{WorldEditClipboard, WorldEditUndo};
use crate::plot::PLOT_SCALE;
use crate::utils::{self, HyphenatedUUID};
use byteorder::{BigEndian, ReadBytesExt};
use mchprs_blocks::block_entities::{ContainerType, InventoryEntry};
use mchprs_blocks::items::{Item, ItemStack};
use mchprs_blocks::{BlockDirection, BlockFacing, BlockPos};
use mchprs_network::packets::{clientbound::*, PlayerProperty};
use mchprs_network::packets::{PacketEncoder, SlotData};
use mchprs_network::{PlayerConn, PlayerPacketSender};
use mchprs_text::{ColorCode, TextComponent, TextComponentBuilder};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{Cursor, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Instant, SystemTime};
use tracing::{error, warn};

pub type EntityId = u32;
static ENTITY_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum Gamemode {
    Creative,
    Spectator,
}

impl Gamemode {
    pub fn get_id(self) -> i32 {
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
    motion: [f64; 3],
    position: [f64; 3],
    rotation: [f32; 2],
    inventory: Vec<InventoryEntry>,
    selected_item_slot: i32,
    fly_speed: f32,
    walk_speed: f32,
    gamemode: Gamemode,
}

impl Default for PlayerData {
    fn default() -> PlayerData {
        PlayerData {
            on_ground: true,
            flying: false,
            motion: [0.0, 0.0, 0.0],
            position: [128.0, 128.0, 128.0],
            rotation: [0.0, 0.0],
            inventory: Vec::new(),
            selected_item_slot: 0,
            fly_speed: 1.0,
            walk_speed: 1.0,
            gamemode: Gamemode::Creative,
        }
    }
}

bitflags! {
    #[derive(Debug)]
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

impl Default for SkinParts {
    fn default() -> Self {
        Self::from_bits(0b1111111).unwrap()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PlayerPos {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl PlayerPos {
    pub fn new(x: f64, y: f64, z: f64) -> PlayerPos {
        PlayerPos { x, y, z }
    }

    pub fn block_pos(self) -> BlockPos {
        BlockPos {
            x: self.x.floor() as i32,
            y: self.y.floor() as i32,
            z: self.z.floor() as i32,
        }
    }

    pub fn chunk_pos(self) -> (i32, i32) {
        (self.x.floor() as i32 >> 4, self.z.floor() as i32 >> 4)
    }

    pub fn plot_pos(self) -> (i32, i32) {
        let (chunk_x, chunk_z) = self.chunk_pos();
        (chunk_x >> PLOT_SCALE, chunk_z >> PLOT_SCALE)
    }
}

impl std::fmt::Display for PlayerPos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {}, {})", self.x, self.y, self.z)
    }
}

pub struct Player {
    pub uuid: u128,
    pub username: String,
    pub properties: Vec<PlayerProperty>,
    pub skin_parts: SkinParts,
    pub inventory: Vec<Option<ItemStack>>,
    /// The selected slot of the player's hotbar (1-9)
    pub selected_slot: u32,
    pub pos: PlayerPos,
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
    pub entity_id: EntityId,
    pub client: PlayerConn,
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
    pub worldedit_redo: Vec<WorldEditUndo>,
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

    fn from_data(
        player_data: PlayerData,
        uuid: u128,
        username: String,
        properties: Vec<PlayerProperty>,
        client: PlayerConn,
    ) -> Player {
        // Load inventory
        let mut inventory: Vec<Option<ItemStack>> = vec![None; 46];
        for entry in player_data.inventory {
            let nbt = entry
                .nbt
                .map(|data| nbt::Blob::from_reader(&mut Cursor::new(data)).unwrap());
            inventory[entry.slot as usize] = Some(ItemStack {
                item_type: Item::from_id(entry.id),
                count: entry.count as u8,
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
            properties,
            skin_parts: Default::default(),
            inventory,
            selected_slot: player_data.selected_item_slot as u32,
            pos: PlayerPos {
                x: player_data.position[0],
                y: player_data.position[1],
                z: player_data.position[2],
            },
            pitch: player_data.rotation[0],
            yaw: player_data.rotation[1],
            last_chunk_x: 0,
            last_chunk_z: 0,
            entity_id: ENTITY_ID_COUNTER.fetch_add(1, Ordering::Relaxed),
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
            worldedit_redo: Vec::new(),
            command_queue: Vec::new(),
            permissions_cache,
        }
    }

    /// This will load the player from the file. If the file does not exist,
    /// It will be created.
    pub fn load_player(
        uuid: u128,
        username: String,
        properties: Vec<PlayerProperty>,
        client: PlayerConn,
    ) -> Player {
        let filename = format!("./world/players/{:032x}", uuid);
        if let Ok(data) = fs::read(&filename) {
            let player_data: PlayerData = match bincode::deserialize(&data) {
                Ok(data) => data,
                Err(_) => {
                    warn!("There was an error loading the player data for {}, player data will be backed up and reset.", username);
                    if let Err(err) = fs::rename(&filename, filename.clone() + ".bak") {
                        error!("Failed to back up player data: {}", err);
                    }
                    return Player::from_data(
                        Default::default(),
                        uuid,
                        username,
                        properties,
                        client,
                    );
                }
            };

            Player::from_data(player_data, uuid, username, properties, client)
        } else {
            Player::from_data(Default::default(), uuid, username, properties, client)
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
            motion: [0f64, 0f64, 0f64],
            on_ground: self.on_ground,
            position: [self.pos.x, self.pos.y, self.pos.z],
            rotation: [self.pitch, self.yaw],
            selected_item_slot: self.selected_slot as i32,
            walk_speed: self.walk_speed,
        })
        .unwrap();
        file.write_all(&data).unwrap();
    }

    /// Manages keep alives and packet reading. Return true if the view position should be updated.
    pub fn update(&mut self) -> bool {
        if self.last_keep_alive_received.elapsed().as_secs() > 30 {
            self.kick("Timed out.".into());
        }
        if self.last_keep_alive_sent.elapsed().as_secs() > 10 {
            self.send_keep_alive();
        }

        // Prevent from locking player position at Infinity or NaN
        if !self.pos.x.is_finite() || !self.pos.y.is_finite() || !self.pos.z.is_finite() {
            self.pos.x = 128.0;
            self.pos.y = 128.0;
            self.pos.z = 128.0;
        }

        let (chunk_x, chunk_z) = self.pos.chunk_pos();
        chunk_x != self.last_chunk_x || chunk_z != self.last_chunk_z
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
        match ((self.yaw / 90.0 + 0.5).floor() as i32 & 3).unsigned_abs() {
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

    pub fn teleport(&mut self, pos: PlayerPos) {
        // Prevent from teleporting to Infinity or NaN
        if !pos.x.is_finite() || !pos.y.is_finite() || !pos.z.is_finite() {
            self.send_error_message("We just saved you from a game crash, don't try it again!");
            return;
        }

        let player_position_and_look = CSynchronizePlayerPosition {
            x: pos.x,
            y: pos.y,
            z: pos.z,
            yaw: 0f32,
            pitch: 0f32,
            flags: 0x08 | 0x10, // pitch and yaw are relative
            teleport_id: 0,
        }
        .encode();
        self.pos = pos;
        self.client.send_packet(&player_position_and_look);
    }

    /// Sends the `ChatMessage` packet containing the raw text component
    /// Position 0: chat (chat box)
    pub fn send_raw_chat(&self, message: TextComponent) {
        let chat_message = CSystemChatMessage {
            content: message,
            overlay: false,
        }
        .encode();
        self.client.send_packet(&chat_message);
    }

    /// Sends a raw chat message to the player
    pub fn send_chat_message(&self, message: &[TextComponent]) {
        let mut component: TextComponent = Default::default();
        component.extra = message.to_vec();
        self.send_raw_chat(component);
    }

    pub fn send_no_permission_message(&self) {
        self.send_error_message("You do not have permission to perform this action.");
    }

    /// Sends the player a light purple system message (`message` is not in json format)
    pub fn send_worldedit_message(&self, message: &str) {
        self.send_raw_system_message(
            TextComponentBuilder::new(message.to_string())
                .color_code(ColorCode::LightPurple)
                .finish(),
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

    pub fn worldedit_send_cui(&self, message: &str) {
        let cui_plugin_message = CPlayPluginMessage {
            channel: String::from("worldedit:cui"),
            data: Vec::from(message.as_bytes()),
        }
        .encode();
        self.client.send_packet(&cui_plugin_message);
    }

    /// Sends the player the disconnect packet, it is still up to the player to end the network stream.
    pub fn kick(&self, reason: TextComponent) {
        let disconnect = CDisconnect { reason }.encode();
        self.client.send_packet(&disconnect);
    }

    pub fn update_player_abilities(&self) {
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
        let change_game_state = CGameEvent {
            reason: CGameEventType::ChangeGamemode,
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

    pub fn open_container(&self, inventory: &[InventoryEntry], container_type: ContainerType) {
        let mut slots: Vec<Option<SlotData>> =
            (0..container_type.num_slots()).map(|_| None).collect();
        for entry in inventory {
            let item_stack = utils::inventory_entry_to_stack(entry);
            slots[entry.slot as usize] = Some(utils::encode_slot_data(&item_stack));
        }

        let open_window = COpenScreen {
            window_id: 1,
            window_type: container_type.window_type() as i32,
            window_title: "Container".into(),
        }
        .encode();
        self.client.send_packet(&open_window);

        let window_items = CSetContainerContent {
            window_id: 1,
            state_id: 0,
            slot_data: slots,
            carried_item: None,
        }
        .encode();
        self.client.send_packet(&window_items);
    }

    pub fn set_inventory_slot(&mut self, slot: u32, item: Option<ItemStack>) {
        let set_slot = CSetContainerSlot {
            window_id: 0,
            state_id: 0,
            slot: slot as i16,
            slot_data: item.as_ref().map(|item| utils::encode_slot_data(item)),
        }
        .encode();
        self.client.send_packet(&set_slot);

        self.inventory[slot as usize] = item;
    }

    pub fn spawn_packet(&self) -> CSpawnEntity {
        CSpawnEntity {
            entity_id: self.entity_id as i32,
            entity_uuid: self.uuid,
            entity_type: 122, // minecraft::player
            pitch: self.pitch,
            yaw: self.yaw,
            // TODO: probably not the same
            head_yaw: self.yaw,
            data: 0, // unused
            x: self.pos.x,
            y: self.pos.y,
            z: self.pos.z,
            velocity_x: 0,
            velocity_y: 0,
            velocity_z: 0,
        }
    }

    pub fn metadata_packet(&self) -> CSetEntityMetadata {
        CSetEntityMetadata {
            entity_id: self.entity_id as i32,
            metadata: vec![
                CSetEntityMetadataEntry {
                    index: 0,
                    metadata_type: 0,
                    value: vec![{
                        let mut bitfield = 0;
                        if self.crouching {
                            bitfield |= 0x02;
                        };
                        if self.sprinting {
                            bitfield |= 0x08;
                        };
                        bitfield
                    }],
                },
                CSetEntityMetadataEntry {
                    index: 6,
                    metadata_type: 20,
                    value: vec![if self.crouching { 5 } else { 0 }],
                },
                CSetEntityMetadataEntry {
                    index: 17,
                    metadata_type: 0,
                    value: vec![self.skin_parts.bits() as u8],
                },
            ],
        }
    }

    pub fn equippment_packet(&self) -> Option<CSetEquipment> {
        self.inventory[self.selected_slot as usize + 36]
            .as_ref()
            .map(|item| {
                CSetEquipment {
                    entity_id: self.entity_id as i32,
                    equipment: vec![CSetEquipmentEquipment {
                        slot: 0, // Main hand
                        item: Some(utils::encode_slot_data(item)),
                    }],
                }
            })
    }
}

pub trait PacketSender {
    fn send_packet(&self, data: &PacketEncoder);

    /// Sends the `ChatMessage` packet containing the raw json data.
    /// Position 1: system message (chat box)
    fn send_raw_system_message(&self, message: TextComponent) {
        let chat_message = CSystemChatMessage {
            content: message,
            overlay: false,
        }
        .encode();
        self.send_packet(&chat_message);
    }

    /// Sends the player a red system message (`message` is not in json format)
    fn send_error_message(&self, message: &str) {
        self.send_raw_system_message(
            TextComponentBuilder::new(message.to_string())
                .color_code(ColorCode::Red)
                .finish(),
        );
    }

    /// Sends the player a yellow system message (`message` is not in json format)
    fn send_system_message(&self, message: &str) {
        self.send_raw_system_message(
            TextComponentBuilder::new(message.to_string())
                .color_code(ColorCode::Yellow)
                .finish(),
        )
    }
}

impl PacketSender for PlayerPacketSender {
    fn send_packet(&self, data: &PacketEncoder) {
        self.send_packet(data);
    }
}

impl PacketSender for Player {
    fn send_packet(&self, data: &PacketEncoder) {
        self.client.send_packet(data);
    }
}
