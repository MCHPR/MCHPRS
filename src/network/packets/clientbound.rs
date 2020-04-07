use super::{PacketEncoder, PacketEncoderExt};
use crate::player::Player;

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
        buf.write_string(32767, self.json_response);
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
        buf.write_string(32767, self.reason);
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
        buf.write_string(36, Player::uuid_with_hyphens(self.uuid));
        buf.write_string(16, self.username);
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

// Play Packets

pub struct C0FChatMessage {
    pub message: String,
    pub position: i8,
}

impl ClientBoundPacket for C0FChatMessage {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_string(32767, self.message);
        buf.write_byte(self.position);
        PacketEncoder::new(buf, 0x0F)
    }
}

pub struct C19PluginMessageBrand {
    pub brand: String,
}

impl ClientBoundPacket for C19PluginMessageBrand {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_string(32767, "minecraft:brand".to_string());
        buf.write_string(32767, self.brand);
        PacketEncoder::new(buf, 0x19)
    }
}

pub struct C1BDisconnect {
    pub reason: String,
}

impl ClientBoundPacket for C1BDisconnect {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_string(32767, self.reason);
        PacketEncoder::new(buf, 0x1B)
    }
}

pub struct C21KeepAlive {
    pub id: i64,
}

impl ClientBoundPacket for C21KeepAlive {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_long(self.id);
        PacketEncoder::new(buf, 0x21)
    }
}

pub struct C22ChunkDataSection {
    pub block_count: i16,
    pub bits_per_block: u8,
    pub palette: Option<Vec<i32>>,
    pub data_array: Vec<u64>,
}

pub struct C22ChunkData {
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub full_chunk: bool,
    pub primary_bit_mask: i32,
    pub heightmaps: nbt::Blob,
    pub chunk_sections: Vec<C22ChunkDataSection>,
    pub biomes: Option<Vec<i32>>,
}

impl ClientBoundPacket for C22ChunkData {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_int(self.chunk_x);
        buf.write_int(self.chunk_z);
        buf.write_boolean(self.full_chunk);
        buf.write_varint(self.primary_bit_mask);
        let mut heightmaps = Vec::new();
        self.heightmaps.to_writer(&mut heightmaps).unwrap();
        buf.write_bytes(&heightmaps);
        if let Some(biomes) = self.biomes {
            for biome in biomes {
                buf.write_int(biome);
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
        buf.write_bytes(&data);
        // Number of block entities
        buf.write_varint(0);
        PacketEncoder::new(buf, 0x22)
    }
}

pub struct C26JoinGame {
    pub entity_id: i32,
    pub gamemode: u8,
    pub dimention: i32,
    pub hash_seed: i64,
    pub max_players: u8,
    pub level_type: String,
    pub view_distance: i32,
    pub reduced_debug_info: bool,
    pub enable_respawn_screen: bool,
}

impl ClientBoundPacket for C26JoinGame {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_int(self.entity_id);
        buf.write_unsigned_byte(self.gamemode);
        buf.write_int(self.dimention);
        buf.write_long(self.hash_seed);
        buf.write_unsigned_byte(self.max_players);
        buf.write_string(16, self.level_type);
        buf.write_varint(self.view_distance);
        buf.write_boolean(self.reduced_debug_info);
        buf.write_boolean(self.enable_respawn_screen);
        PacketEncoder::new(buf, 0x26)
    }
}

pub struct C34PlayerInfoAddPlayerProperty {
    name: String,
    value: String,
    signature: Option<String>,
}

pub struct C34PlayerInfoAddPlayer {
    pub uuid: u128,
    pub name: String,
    pub properties: Vec<C34PlayerInfoAddPlayerProperty>,
    pub gamemode: i32,
    pub ping: i32,
    pub display_name: Option<String>,
}

pub enum C34PlayerInfo {
    AddPlayer(Vec<C34PlayerInfoAddPlayer>),
    RemovePlayer(Vec<u128>),
}

impl ClientBoundPacket for C34PlayerInfo {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        match self {
            C34PlayerInfo::AddPlayer(ps) => {
                buf.write_varint(0);
                buf.write_varint(ps.len() as i32);
                for p in ps {
                    buf.write_uuid(p.uuid);
                    buf.write_string(16, p.name);
                    buf.write_varint(p.properties.len() as i32);
                    for prop in p.properties {
                        buf.write_string(32767, prop.name);
                        buf.write_string(32767, prop.value);
                        buf.write_boolean(prop.signature.is_some());
                        if let Some(signature) = prop.signature {
                            buf.write_string(32767, signature);
                        }
                    }
                    buf.write_varint(p.gamemode);
                    buf.write_varint(p.ping);
                    buf.write_boolean(p.display_name.is_some());
                    if let Some(display_name) = p.display_name {
                        buf.write_string(32767, display_name);
                    }
                }
            }
            C34PlayerInfo::RemovePlayer(uuids) => {
                buf.write_varint(4);
                buf.write_varint(uuids.len() as i32);
                for uuid in uuids {
                    buf.write_uuid(uuid);
                }
            }
        }
        PacketEncoder::new(buf, 0x34)
    }
}

pub struct C36PlayerPositionAndLook {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
    pub flags: u8,
    pub teleport_id: i32,
}

impl ClientBoundPacket for C36PlayerPositionAndLook {
    fn encode(self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_double(self.x);
        buf.write_double(self.y);
        buf.write_double(self.z);
        buf.write_float(self.yaw);
        buf.write_float(self.pitch);
        buf.write_unsigned_byte(self.flags);
        buf.write_varint(self.teleport_id);
        PacketEncoder::new(buf, 0x36)
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
            buf.write_bytes(&entry.value);
        }
        buf.write_byte(-1); // 0xFF
        PacketEncoder::new(buf, 0x44)
    }
}
