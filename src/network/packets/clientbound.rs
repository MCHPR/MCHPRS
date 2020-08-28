use super::{PacketEncoder, PacketEncoderExt, SlotData};
use serde::Serialize;

pub trait ClientBoundPacket {
    fn encode(self) -> PacketEncoder;
}

// Server List Ping Packets

pub struct C00Response {
    pub json_response: String,
}

impl ClientBoundPacket for C00Response {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_string(32767, &self.json_response);
        PacketEncoder::new(buf, 0x00)
    }
}

// Login Packets

pub struct C00DisconnectLogin {
    pub reason: String,
}

impl ClientBoundPacket for C00DisconnectLogin {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_string(32767, &self.reason);
        PacketEncoder::new(buf, 0x00)
    }
}

pub struct C01Pong {
    pub payload: i64,
}

impl ClientBoundPacket for C01Pong {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_long(self.payload);
        PacketEncoder::new(buf, 0x01)
    }
}

pub struct C02LoginSuccess {
    pub uuid: u128,
    pub username: String,
}

impl ClientBoundPacket for C02LoginSuccess {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_uuid(self.uuid);
        buf.write_string(16, &self.username);
        PacketEncoder::new(buf, 0x02)
    }
}

pub struct C03SetCompression {
    pub threshold: i32,
}

impl ClientBoundPacket for C03SetCompression {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.threshold);
        PacketEncoder::new(buf, 0x03)
    }
}

pub struct C04SpawnPlayer {
    pub entity_id: i32,
    pub uuid: u128,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
}

impl ClientBoundPacket for C04SpawnPlayer {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.entity_id);
        buf.write_uuid(self.uuid);
        buf.write_double(self.x);
        buf.write_double(self.y);
        buf.write_double(self.z);
        buf.write_byte((self.yaw % 350f32 / 350f32 * 256f32) as i8);
        buf.write_byte((self.pitch % 350f32 / 350f32 * 256f32) as i8);
        PacketEncoder::new(buf, 0x04)
    }
}

// Play Packets

pub struct C05EntityAnimation {
    pub entity_id: i32,
    pub animation: u8,
}

impl ClientBoundPacket for C05EntityAnimation {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.entity_id);
        buf.write_unsigned_byte(self.animation);
        PacketEncoder::new(buf, 0x05)
    }
}

pub struct C09BlockEntityData {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub action: u8,
    pub nbt: nbt::Blob,
}

impl ClientBoundPacket for C09BlockEntityData {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_position(self.x, self.y, self.z);
        buf.write_unsigned_byte(self.action);
        buf.write_nbt_blob(self.nbt);
        PacketEncoder::new(buf, 0x09)
    }
}

pub struct C0BBlockChange {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub block_id: i32,
}

impl ClientBoundPacket for C0BBlockChange {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_position(self.x, self.y, self.z);
        buf.write_varint(self.block_id);
        PacketEncoder::new(buf, 0x0B)
    }
}

pub struct C0EChatMessage {
    pub message: String,
    pub position: i8,
    pub sender: u128,
}

impl ClientBoundPacket for C0EChatMessage {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_string(32767, &self.message);
        buf.write_byte(self.position);
        buf.write_uuid(self.sender);
        PacketEncoder::new(buf, 0x0E)
    }
}

pub enum C10DeclareCommandsNodeParser {
    Entity(i8),
    Vec3,
    Integer(i32, i32),
    Float(f32, f32),
    BlockPos,
    BlockState,
}

impl C10DeclareCommandsNodeParser {
    fn write(&self, buf: &mut Vec<u8>) {
        use C10DeclareCommandsNodeParser::*;
        match self {
            Entity(flags) => {
                buf.write_string(32767, "minecraft:entity");
                buf.write_byte(*flags);
            }
            Vec3 => buf.write_string(32767, "minecraft:vec3"),
            BlockPos => buf.write_string(32767, "minecraft:block_pos"),
            BlockState => buf.write_string(32767, "minecraft:block_state"),
            Integer(min, max) => {
                buf.write_string(32767, "brigadier:integer");
                buf.write_byte(3); // Supply min and max value
                buf.write_int(*min);
                buf.write_int(*max);
            }
            Float(min, max) => {
                buf.write_string(32767, "brigadier:float");
                buf.write_byte(3);
                buf.write_float(*min);
                buf.write_float(*max);
            }
        }
    }
}

pub struct C10DeclareCommandsNode {
    pub flags: i8,
    pub children: Vec<i32>,
    pub redirect_node: Option<i32>,
    pub name: Option<&'static str>,
    pub parser: Option<C10DeclareCommandsNodeParser>,
}

pub struct C10DeclareCommands {
    pub nodes: Vec<C10DeclareCommandsNode>,
    pub root_index: i32,
}

impl ClientBoundPacket for C10DeclareCommands {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.nodes.len() as i32);
        for node in self.nodes {
            buf.write_byte(node.flags);
            buf.write_varint(node.children.len() as i32);
            for child in node.children {
                buf.write_varint(child);
            }
            if let Some(redirect_node) = node.redirect_node {
                buf.write_varint(redirect_node);
            }
            if let Some(name) = node.name {
                buf.write_string(32767, &name);
            }
            if let Some(parser) = node.parser {
                parser.write(&mut buf);
            }
        }
        buf.write_varint(self.root_index);
        PacketEncoder::new(buf, 0x10)
    }
}

pub struct C13WindowItems {
    pub window_id: u8,
    pub slot_data: Vec<Option<SlotData>>,
}

impl ClientBoundPacket for C13WindowItems {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_unsigned_byte(self.window_id);
        buf.write_short(self.slot_data.len() as i16);
        for slot_data in self.slot_data {
            if let Some(slot) = slot_data {
                buf.write_bool(true);
                buf.write_varint(slot.item_id);
                buf.write_byte(slot.item_count);
                if let Some(nbt) = slot.nbt {
                    buf.write_nbt_blob(nbt);
                } else {
                    buf.write_byte(0); // End tag
                }
            } else {
                buf.write_bool(false);
            }
        }
        PacketEncoder::new(buf, 0x13)
    }
}

pub struct C17PluginMessageBrand {
    pub brand: String,
}

impl ClientBoundPacket for C17PluginMessageBrand {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_string(32767, "minecraft:brand");
        buf.write_string(32767, &self.brand);
        PacketEncoder::new(buf, 0x17)
    }
}

pub struct C19Disconnect {
    pub reason: String,
}

impl ClientBoundPacket for C19Disconnect {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_string(32767, &self.reason);
        PacketEncoder::new(buf, 0x19)
    }
}

#[derive(Debug)]
pub struct C1CUnloadChunk {
    pub chunk_x: i32,
    pub chunk_z: i32,
}

impl ClientBoundPacket for C1CUnloadChunk {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_int(self.chunk_x);
        buf.write_int(self.chunk_z);
        PacketEncoder::new(buf, 0x1C)
    }
}

pub struct C1FKeepAlive {
    pub id: i64,
}

impl ClientBoundPacket for C1FKeepAlive {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_long(self.id);
        PacketEncoder::new(buf, 0x1F)
    }
}

pub struct C20ChunkDataSection {
    pub block_count: i16,
    pub bits_per_block: u8,
    pub palette: Option<Vec<i32>>,
    pub data_array: Vec<u64>,
}

pub struct C20ChunkData {
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub full_chunk: bool,
    pub primary_bit_mask: i32,
    pub heightmaps: nbt::Blob,
    pub biomes: Option<Vec<i32>>,
    pub chunk_sections: Vec<C20ChunkDataSection>,
    pub block_entities: Vec<nbt::Blob>,
}

impl ClientBoundPacket for C20ChunkData {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_int(self.chunk_x);
        buf.write_int(self.chunk_z);
        buf.write_boolean(self.full_chunk);
        buf.write_varint(self.primary_bit_mask);
        let mut heightmaps = Vec::new();
        self.heightmaps.to_writer(&mut heightmaps).unwrap();
        buf.write_bytes(heightmaps);
        if let Some(biomes) = self.biomes {
            buf.write_varint(biomes.len() as i32);
            for biome in biomes {
                buf.write_varint(biome);
            }
        }
        let mut data = Vec::new();
        for chunk_section in self.chunk_sections {
            data.write_short(chunk_section.block_count);
            data.write_unsigned_byte(chunk_section.bits_per_block);
            if let Some(palette) = chunk_section.palette {
                data.write_varint(palette.len() as i32);
                for palette_entry in palette {
                    data.write_varint(palette_entry);
                }
            }
            data.write_varint(chunk_section.data_array.len() as i32);
            for long in chunk_section.data_array {
                data.write_long(long as i64);
            }
        }
        buf.write_varint(data.len() as i32);
        buf.write_bytes(data);
        // Number of block entities
        buf.write_varint(self.block_entities.len() as i32);
        for block_entity in self.block_entities {
            buf.write_nbt_blob(block_entity);
        }
        PacketEncoder::new(buf, 0x20)
    }
}

pub struct C21Effect {
    pub effect_id: i32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub data: i32,
    pub disable_relative_volume: bool,
}

impl ClientBoundPacket for C21Effect {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_int(self.effect_id);
        buf.write_position(self.x, self.y, self.z);
        buf.write_int(self.data);
        buf.write_bool(self.disable_relative_volume);
        PacketEncoder::new(buf, 0x21)
    }
}

#[derive(Serialize)]
pub struct C24JoinGameDimensionCodecDimension {
    pub name: String,
    pub natural: i8,
    pub ambient_light: f32,
    pub has_ceiling: i8,
    pub has_skylight: i8,
    pub fixed_time: i64,
    pub shrunk: i8,
    pub ultrawarm: i8,
    pub has_raids: i8,
    pub respawn_anchor_works: i8,
    pub bed_works: i8,
    pub piglin_safe: i8,
    pub logical_height: i32,
    pub infiniburn: String,
}

#[derive(Serialize)]
pub struct C24JoinGameDimensionCodecBiome {
    pub name: String,
    pub natural: i8,
    pub ambient_light: f32,
    pub has_ceiling: i8,
    pub has_skylight: i8,
    pub fixed_time: i64,
    pub shrunk: i8,
    pub ultrawarm: i8,
    pub has_raids: i8,
    pub respawn_anchor_works: i8,
    pub bed_works: i8,
    pub piglin_safe: i8,
    pub logical_height: i32,
    pub infiniburn: String,
}

#[derive(Serialize)]
pub struct C24JoinGameDimensionCodec {
    pub dimension: Vec<C24JoinGameDimensionCodecDimension>,
}

pub struct C24JoinGame {
    pub entity_id: i32,
    pub is_hardcore: bool,
    pub gamemode: u8,
    pub previous_gamemode: u8,
    pub world_count: i32,
    pub world_names: Vec<String>,
    pub dimention_codec: C24JoinGameDimensionCodec,
    pub dimention: String,
    pub world_name: String,
    pub hashed_seed: i64,
    pub max_players: i32,
    pub view_distance: i32,
    pub reduced_debug_info: bool,
    pub enable_respawn_screen: bool,
    pub is_debug: bool,
    pub is_flat: bool,
}

impl ClientBoundPacket for C24JoinGame {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_int(self.entity_id);
        buf.write_bool(self.is_hardcore);
        buf.write_unsigned_byte(self.gamemode);
        buf.write_unsigned_byte(self.previous_gamemode);
        buf.write_varint(self.world_count);
        for world_name in self.world_names {
            buf.write_string(32767, &world_name);
        }
        buf.write_nbt(self.dimention_codec);
        buf.write_string(32767, &self.dimention);
        buf.write_string(32767, &self.world_name);
        buf.write_long(self.hashed_seed);
        buf.write_varint(self.max_players);
        buf.write_varint(self.view_distance);
        buf.write_boolean(self.reduced_debug_info);
        buf.write_boolean(self.enable_respawn_screen);
        buf.write_boolean(self.is_debug);
        buf.write_boolean(self.is_flat);
        PacketEncoder::new(buf, 0x24)
    }
}

pub struct C27EntityPosition {
    pub entity_id: i32,
    pub delta_x: i16,
    pub delta_y: i16,
    pub delta_z: i16,
    pub on_ground: bool,
}

impl ClientBoundPacket for C27EntityPosition {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.entity_id);
        buf.write_short(self.delta_x);
        buf.write_short(self.delta_y);
        buf.write_short(self.delta_z);
        buf.write_bool(self.on_ground);
        PacketEncoder::new(buf, 0x27)
    }
}

pub struct C28EntityPositionAndRotation {
    pub entity_id: i32,
    pub delta_x: i16,
    pub delta_y: i16,
    pub delta_z: i16,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
}

impl ClientBoundPacket for C28EntityPositionAndRotation {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.entity_id);
        buf.write_short(self.delta_x);
        buf.write_short(self.delta_y);
        buf.write_short(self.delta_z);
        buf.write_byte((self.yaw % 350f32 / 350f32 * 256f32) as i8);
        buf.write_byte((self.pitch % 350f32 / 350f32 * 256f32) as i8);
        buf.write_bool(self.on_ground);
        PacketEncoder::new(buf, 0x28)
    }
}

pub struct C29EntityRotation {
    pub entity_id: i32,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
}

impl ClientBoundPacket for C29EntityRotation {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.entity_id);
        buf.write_byte((self.yaw % 350f32 / 350f32 * 256f32) as i8);
        buf.write_byte((self.pitch % 350f32 / 350f32 * 256f32) as i8);
        buf.write_bool(self.on_ground);
        PacketEncoder::new(buf, 0x29)
    }
}

pub struct C2AEntityMovement {
    pub entity_id: i32,
}

impl ClientBoundPacket for C2AEntityMovement {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.entity_id);
        PacketEncoder::new(buf, 0x2A)
    }
}

pub struct C30PlayerAbilities {
    pub flags: u8,
    pub fly_speed: f32,
    pub fov_modifier: f32,
}

impl ClientBoundPacket for C30PlayerAbilities {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_unsigned_byte(self.flags);
        buf.write_float(self.fly_speed);
        buf.write_float(self.fov_modifier);
        PacketEncoder::new(buf, 0x30)
    }
}

pub struct C32PlayerInfoAddPlayerProperty {
    name: String,
    value: String,
    signature: Option<String>,
}

pub struct C32PlayerInfoAddPlayer {
    pub uuid: u128,
    pub name: String,
    pub properties: Vec<C32PlayerInfoAddPlayerProperty>,
    pub gamemode: i32,
    pub ping: i32,
    pub display_name: Option<String>,
}

pub enum C32PlayerInfo {
    AddPlayer(Vec<C32PlayerInfoAddPlayer>),
    RemovePlayer(Vec<u128>),
}

impl ClientBoundPacket for C32PlayerInfo {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        match self {
            C32PlayerInfo::AddPlayer(ps) => {
                buf.write_varint(0);
                buf.write_varint(ps.len() as i32);
                for p in ps {
                    buf.write_uuid(p.uuid);
                    buf.write_string(16, &p.name);
                    buf.write_varint(p.properties.len() as i32);
                    for prop in p.properties {
                        buf.write_string(32767, &prop.name);
                        buf.write_string(32767, &prop.value);
                        buf.write_boolean(prop.signature.is_some());
                        if let Some(signature) = prop.signature {
                            buf.write_string(32767, &signature);
                        }
                    }
                    buf.write_varint(p.gamemode);
                    buf.write_varint(p.ping);
                    buf.write_boolean(p.display_name.is_some());
                    if let Some(display_name) = p.display_name {
                        buf.write_string(32767, &display_name);
                    }
                }
            }
            C32PlayerInfo::RemovePlayer(uuids) => {
                buf.write_varint(4);
                buf.write_varint(uuids.len() as i32);
                for uuid in uuids {
                    buf.write_uuid(uuid);
                }
            }
        }
        PacketEncoder::new(buf, 0x32)
    }
}

pub struct C34PlayerPositionAndLook {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
    pub flags: u8,
    pub teleport_id: i32,
}

impl ClientBoundPacket for C34PlayerPositionAndLook {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_double(self.x);
        buf.write_double(self.y);
        buf.write_double(self.z);
        buf.write_float(self.yaw);
        buf.write_float(self.pitch);
        buf.write_unsigned_byte(self.flags);
        buf.write_varint(self.teleport_id);
        PacketEncoder::new(buf, 0x35)
    }
}

pub struct C36DestroyEntities {
    pub entity_ids: Vec<i32>,
}

impl ClientBoundPacket for C36DestroyEntities {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.entity_ids.len() as i32);
        for entity_id in self.entity_ids {
            buf.write_varint(entity_id);
        }
        PacketEncoder::new(buf, 0x36)
    }
}

pub struct C3AEntityHeadLook {
    pub entity_id: i32,
    pub yaw: f32,
}

impl ClientBoundPacket for C3AEntityHeadLook {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.entity_id);
        buf.write_byte((self.yaw % 350f32 / 350f32 * 256f32) as i8);
        PacketEncoder::new(buf, 0x3A)
    }
}

pub struct C3BMultiBlockChangeRecord {
    pub x: i8,
    pub y: u8,
    pub z: i8,
    pub block_id: i32,
}

pub struct C3BMultiBlockChange {
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub records: Vec<C3BMultiBlockChangeRecord>,
}

impl ClientBoundPacket for C3BMultiBlockChange {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();

        buf.write_int(self.chunk_x);
        buf.write_int(self.chunk_z);
        buf.write_varint(self.records.len() as i32); // Length of record array

        for record in self.records {
            buf.write_byte((record.x << 4) | (record.z & 0x0F)); // 0bXXXXZZZZ
            buf.write_unsigned_byte(record.y);
            buf.write_varint(record.block_id);
        }

        PacketEncoder::new(buf, 0x3B)
    }
}

pub struct C3FHeldItemChange {
    pub slot: i8,
}

impl ClientBoundPacket for C3FHeldItemChange {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_byte(self.slot);
        PacketEncoder::new(buf, 0x3F)
    }
}

pub struct C40UpdateViewPosition {
    pub chunk_x: i32,
    pub chunk_z: i32,
}

impl ClientBoundPacket for C40UpdateViewPosition {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.chunk_x);
        buf.write_varint(self.chunk_z);
        PacketEncoder::new(buf, 0x40)
    }
}

pub struct C44EntityMetadataEntry {
    pub index: u8,
    pub metadata_type: i32,
    pub value: Vec<u8>,
}

pub struct C44EntityMetadata {
    pub entity_id: i32,
    pub metadata: Vec<C44EntityMetadataEntry>,
}

impl ClientBoundPacket for C44EntityMetadata {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.entity_id);
        for entry in self.metadata {
            buf.write_unsigned_byte(entry.index);
            buf.write_varint(entry.metadata_type);
            buf.write_bytes(entry.value);
        }
        buf.write_byte(-1); // 0xFF
        PacketEncoder::new(buf, 0x44)
    }
}

pub struct C47EntityEquipmentEquipment {
    pub slot: i32,
    pub item: Option<SlotData>,
}

pub struct C47EntityEquipment {
    pub entity_id: i32,
    pub equipment: Vec<C47EntityEquipmentEquipment>,
}

impl ClientBoundPacket for C47EntityEquipment {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.entity_id);
        for slot in self.equipment {
            buf.write_varint(slot.slot);
            if let Some(slot) = slot.item {
                buf.write_bool(true);
                buf.write_varint(slot.item_id);
                buf.write_byte(slot.item_count);
                if let Some(nbt) = slot.nbt {
                    buf.write_nbt_blob(nbt);
                } else {
                    buf.write_byte(0); // End tag
                }
            } else {
                buf.write_bool(false);
            }
        }

        PacketEncoder::new(buf, 0x47)
    }
}

pub struct C4ETimeUpdate {
    pub world_age: i64,
    pub time_of_day: i64,
}

impl ClientBoundPacket for C4ETimeUpdate {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_long(self.world_age);
        buf.write_long(self.time_of_day);
        PacketEncoder::new(buf, 0x4E)
    }
}

pub struct C56EntityTeleport {
    pub entity_id: i32,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
}

impl ClientBoundPacket for C56EntityTeleport {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.entity_id);
        buf.write_double(self.x);
        buf.write_double(self.y);
        buf.write_double(self.z);
        buf.write_byte((self.yaw % 350f32 / 350f32 * 256f32) as i8);
        buf.write_byte((self.pitch % 350f32 / 350f32 * 256f32) as i8);
        buf.write_bool(self.on_ground);
        PacketEncoder::new(buf, 0x56)
    }
}
