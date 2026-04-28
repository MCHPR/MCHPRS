use super::{PacketEncoder, PacketEncoderExt, PalettedContainer, PlayerProperty, SlotData};
use crate::nbt_util::{NBTCompound, NBTMap};
use bitvec::bits;
use bitvec::prelude::Lsb0;
use mchprs_proc_macros::{packet_id, protocol_id};
use mchprs_text::TextComponent;
use serde::Serialize;
use std::collections::HashMap;

pub trait ClientBoundPacket {
    fn encode(&self) -> PacketEncoder;
}

fn encode_plugin_message(packet_id: u32, channel: &str, data: &[u8]) -> PacketEncoder {
    let mut buf = Vec::new();
    buf.write_string(32767, channel);
    buf.write_bytes(data);
    PacketEncoder::new(buf, packet_id)
}

// Server List Ping Packets

pub struct CStatusReponse {
    pub json_response: String,
}

impl ClientBoundPacket for CStatusReponse {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_string(32767, &self.json_response);
        PacketEncoder::new(
            buf,
            packet_id!("status", "clientbound", "minecraft:status_response"),
        )
    }
}

pub struct CPong {
    pub payload: i64,
}

impl ClientBoundPacket for CPong {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_long(self.payload);
        PacketEncoder::new(
            buf,
            packet_id!("status", "clientbound", "minecraft:pong_response"),
        )
    }
}

// Login Packets

pub struct CDisconnectLogin {
    pub reason: String,
}

impl ClientBoundPacket for CDisconnectLogin {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_string(32767, &self.reason);
        PacketEncoder::new(
            buf,
            packet_id!("login", "clientbound", "minecraft:login_disconnect"),
        )
    }
}

pub struct CLoginSuccess {
    pub uuid: u128,
    pub username: String,
    pub properties: Vec<PlayerProperty>,
}

impl ClientBoundPacket for CLoginSuccess {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_uuid(self.uuid);
        buf.write_string(16, &self.username);
        buf.write_varint(self.properties.len() as i32);
        for prop in &self.properties {
            buf.write_player_property(prop);
        }
        PacketEncoder::new(
            buf,
            packet_id!("login", "clientbound", "minecraft:login_finished"),
        )
    }
}

pub struct CSetCompression {
    pub threshold: i32,
}

impl ClientBoundPacket for CSetCompression {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.threshold);
        PacketEncoder::new(
            buf,
            packet_id!("login", "clientbound", "minecraft:login_compression"),
        )
    }
}

pub struct CLoginPluginRequest {
    pub message_id: i32,
    pub channel: String,
    pub data: Vec<u8>,
}

impl ClientBoundPacket for CLoginPluginRequest {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.message_id);
        buf.write_identifier(&self.channel);
        buf.write_bytes(&self.data);
        PacketEncoder::new(
            buf,
            packet_id!("login", "clientbound", "minecraft:custom_query"),
        )
    }
}

// Configuration Packets

pub struct CConfigurationPluginMessage {
    pub channel: String,
    pub data: Vec<u8>,
}

impl ClientBoundPacket for CConfigurationPluginMessage {
    fn encode(&self) -> PacketEncoder {
        encode_plugin_message(
            packet_id!("configuration", "clientbound", "minecraft:custom_payload"),
            &self.channel,
            &self.data,
        )
    }
}

pub struct CFinishConfiguration;

impl ClientBoundPacket for CFinishConfiguration {
    fn encode(&self) -> PacketEncoder {
        PacketEncoder::new(
            Vec::new(),
            packet_id!(
                "configuration",
                "clientbound",
                "minecraft:finish_configuration"
            ),
        )
    }
}

pub struct CRegistryDataEntry<'a> {
    pub entry_id: &'a str,
    pub data: Option<nbt::Blob>,
}

impl<'a> CRegistryDataEntry<'a> {
    pub fn new(entry_id: &'a str) -> Self {
        Self {
            entry_id,
            data: None,
        }
    }

    pub fn with_nbt(entry_id: &'a str, nbt: nbt::Blob) -> Self {
        Self {
            entry_id,
            data: Some(nbt),
        }
    }
}

pub struct CRegistryData<'a> {
    pub registry_id: &'a str,
    pub entries: &'a [CRegistryDataEntry<'a>],
}

impl<'a> CRegistryData<'a> {
    pub fn new(registry_id: &'a str, entries: &'a [CRegistryDataEntry]) -> Self {
        Self {
            registry_id,
            entries,
        }
    }
}

impl ClientBoundPacket for CRegistryData<'_> {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_identifier(self.registry_id);

        buf.write_varint(self.entries.len() as i32);
        for entry in self.entries {
            buf.write_identifier(entry.entry_id);

            buf.write_bool(entry.data.is_some());
            if let Some(nbt) = entry.data.as_ref() {
                buf.write_nbt(nbt);
            }
        }

        PacketEncoder::new(
            buf,
            packet_id!("configuration", "clientbound", "minecraft:registry_data"),
        )
    }
}

pub struct CKnownPacksEntry<'a> {
    pub namespace: &'a str,
    pub id: &'a str,
    pub version: &'a str,
}

pub struct CKnownPacks<'a> {
    pub known_packs: &'a [CKnownPacksEntry<'a>],
}

impl ClientBoundPacket for CKnownPacks<'_> {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.known_packs.len() as i32);
        for known_pack in self.known_packs {
            buf.write_identifier(known_pack.namespace);
            buf.write_identifier(known_pack.id);
            buf.write_identifier(known_pack.version);
        }
        PacketEncoder::new(
            buf,
            packet_id!(
                "configuration",
                "clientbound",
                "minecraft:select_known_packs"
            ),
        )
    }
}

// Play Packets

pub struct CSpawnEntity {
    pub entity_id: i32,
    pub entity_uuid: u128,
    pub entity_type: i32,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub pitch: f32,
    pub yaw: f32,
    pub head_yaw: f32,
    pub data: i32,
    pub velocity_x: i16,
    pub velocity_y: i16,
    pub velocity_z: i16,
}

impl ClientBoundPacket for CSpawnEntity {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.entity_id);
        buf.write_uuid(self.entity_uuid);
        buf.write_varint(self.entity_type);
        buf.write_double(self.x);
        buf.write_double(self.y);
        buf.write_double(self.z);
        buf.write_byte(((self.pitch / 360f32 * 256f32) as i32 % 256) as i8);
        buf.write_byte(((self.yaw / 360f32 * 256f32) as i32 % 256) as i8);
        buf.write_byte(((self.head_yaw / 360f32 * 256f32) as i32 % 256) as i8);
        buf.write_varint(self.data);
        buf.write_short(self.velocity_x);
        buf.write_short(self.velocity_y);
        buf.write_short(self.velocity_z);
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:add_entity"),
        )
    }
}

pub struct CEntityAnimation {
    pub entity_id: i32,
    pub animation: u8,
}

impl ClientBoundPacket for CEntityAnimation {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.entity_id);
        buf.write_unsigned_byte(self.animation);
        PacketEncoder::new(buf, packet_id!("play", "clientbound", "minecraft:animate"))
    }
}

pub struct CAcknowledgeBlockChange {
    pub sequence_id: i32,
}

impl ClientBoundPacket for CAcknowledgeBlockChange {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.sequence_id);
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:block_changed_ack"),
        )
    }
}

pub struct CBlockEntityData {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub ty: i32,
    pub nbt: NBTCompound,
}

impl ClientBoundPacket for CBlockEntityData {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_position(self.x, self.y, self.z);
        buf.write_varint(self.ty);
        buf.write_nbt(&self.nbt);
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:block_entity_data"),
        )
    }
}

pub struct CBlockUpdate {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub block_id: i32,
}

impl ClientBoundPacket for CBlockUpdate {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_position(self.x, self.y, self.z);
        buf.write_varint(self.block_id);
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:block_update"),
        )
    }
}

pub struct CCommandSuggestionsResponseMatch {
    pub match_: String,
    pub tooltip: Option<TextComponent>,
}

pub struct CCommandSuggestionsResponse {
    pub id: i32,
    pub start: i32,
    pub length: i32,
    pub matches: Vec<CCommandSuggestionsResponseMatch>,
}

impl ClientBoundPacket for CCommandSuggestionsResponse {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.id);
        buf.write_varint(self.start);
        buf.write_varint(self.length);
        buf.write_varint(self.matches.len() as i32);
        for m in &self.matches {
            buf.write_string(32767, &m.match_);
            buf.write_bool(m.tooltip.is_some());
            if let Some(tooltip) = &m.tooltip {
                buf.write_text_component(tooltip);
            }
        }

        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:command_suggestions"),
        )
    }
}

#[derive(Debug)]
pub enum CDeclareCommandsNodeParser {
    Entity(i8),
    Vec2,
    Vec3,
    Integer(i32, i32),
    Float(f32, f32),
    BlockPos,
    BlockState,
    String(i32),
}

impl CDeclareCommandsNodeParser {
    fn write(&self, buf: &mut Vec<u8>) {
        use CDeclareCommandsNodeParser::*;
        macro_rules! arg_type {
            ($name:literal) => {
                protocol_id!("minecraft:command_argument_type", $name)
            };
        }
        match self {
            Entity(flags) => {
                buf.write_varint(arg_type!("minecraft:entity"));
                buf.write_byte(*flags);
            }
            Vec2 => buf.write_varint(arg_type!("minecraft:vec2")),
            Vec3 => buf.write_varint(arg_type!("minecraft:vec3")),
            BlockPos => buf.write_varint(arg_type!("minecraft:block_pos")),
            BlockState => buf.write_varint(arg_type!("minecraft:block_state")),
            Integer(min, max) => {
                buf.write_varint(arg_type!("brigadier:integer"));
                buf.write_byte(3); // Supply min and max value
                buf.write_int(*min);
                buf.write_int(*max);
            }
            Float(min, max) => {
                buf.write_varint(arg_type!("brigadier:float"));
                buf.write_byte(3);
                buf.write_float(*min);
                buf.write_float(*max);
            }
            String(ty) => {
                buf.write_varint(arg_type!("brigadier:string"));
                buf.write_varint(*ty);
            }
        }
    }
}

#[derive(Debug)]
pub struct CCommandsNode {
    pub flags: i8,
    pub children: Vec<i32>,
    pub redirect_node: Option<i32>,
    pub name: Option<&'static str>,
    pub parser: Option<CDeclareCommandsNodeParser>,
    pub suggestions_type: Option<&'static str>,
}

pub struct CCommands {
    pub nodes: Vec<CCommandsNode>,
    pub root_index: i32,
}

impl ClientBoundPacket for CCommands {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.nodes.len() as i32);
        for node in &self.nodes {
            buf.write_byte(node.flags);
            buf.write_varint(node.children.len() as i32);
            for child in &node.children {
                buf.write_varint(*child);
            }
            if let Some(redirect_node) = node.redirect_node {
                buf.write_varint(redirect_node);
            }
            if let Some(name) = node.name {
                buf.write_string(32767, name);
            }
            if let Some(parser) = &node.parser {
                parser.write(&mut buf);
            }
            if let Some(suggesstions_type) = node.suggestions_type {
                buf.write_string(32767, suggesstions_type);
            }
        }
        buf.write_varint(self.root_index);
        PacketEncoder::new(buf, packet_id!("play", "clientbound", "minecraft:commands"))
    }
}

pub struct CSetContainerContent {
    pub window_id: i32,
    pub state_id: i32,
    pub slot_data: Vec<Option<SlotData>>,
    pub carried_item: Option<SlotData>,
}

impl ClientBoundPacket for CSetContainerContent {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.window_id);
        buf.write_varint(self.state_id);
        buf.write_varint(self.slot_data.len() as i32);
        for slot_data in &self.slot_data {
            buf.write_slot_data(slot_data);
        }
        buf.write_slot_data(&self.carried_item);
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:container_set_content"),
        )
    }
}

pub struct CSetContainerSlot {
    pub window_id: i32,
    pub state_id: i32,
    pub slot: i16,
    pub slot_data: Option<SlotData>,
}

impl ClientBoundPacket for CSetContainerSlot {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.window_id);
        buf.write_varint(self.state_id);
        buf.write_short(self.slot);
        buf.write_slot_data(&self.slot_data);
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:container_set_slot"),
        )
    }
}

pub struct CPlayPluginMessage {
    pub channel: String,
    pub data: Vec<u8>,
}

impl ClientBoundPacket for CPlayPluginMessage {
    fn encode(&self) -> PacketEncoder {
        encode_plugin_message(
            packet_id!("play", "clientbound", "minecraft:custom_payload"),
            &self.channel,
            &self.data,
        )
    }
}

pub struct CDisconnect {
    pub reason: TextComponent,
}

impl ClientBoundPacket for CDisconnect {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_text_component(&self.reason);
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:disconnect"),
        )
    }
}

#[derive(Debug)]
pub struct CUnloadChunk {
    pub chunk_x: i32,
    pub chunk_z: i32,
}

impl ClientBoundPacket for CUnloadChunk {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_int(self.chunk_z);
        buf.write_int(self.chunk_x);
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:forget_level_chunk"),
        )
    }
}

pub enum CGameEventType {
    ChangeGamemode,
    /// Start waiting for level chunks
    WaitForChunks,
}

pub struct CGameEvent {
    pub reason: CGameEventType,
    pub value: f32,
}

impl ClientBoundPacket for CGameEvent {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        match self.reason {
            CGameEventType::ChangeGamemode => buf.write_unsigned_byte(3),
            CGameEventType::WaitForChunks => buf.write_unsigned_byte(13),
        }
        buf.write_float(self.value);
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:game_event"),
        )
    }
}

pub struct CKeepAlive {
    pub id: i64,
}

impl ClientBoundPacket for CKeepAlive {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_long(self.id);
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:keep_alive"),
        )
    }
}

pub struct CChunkDataSection {
    pub block_count: i16,
    pub block_states: PalettedContainer,
    pub biomes: PalettedContainer,
}

pub struct CChunkDataBlockEntity {
    pub x: i8,
    pub z: i8,
    pub y: i16,
    pub ty: i32,
    pub data: NBTCompound,
}

/// Chunk Data and Update Light
pub struct CChunkData {
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub heightmaps: HashMap<i32, Vec<i64>>,
    pub chunk_sections: Vec<CChunkDataSection>,
    pub block_entities: Vec<CChunkDataBlockEntity>,
}

impl ClientBoundPacket for CChunkData {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_int(self.chunk_x);
        buf.write_int(self.chunk_z);

        buf.write_varint(self.heightmaps.len() as i32);
        for (ty, data) in &self.heightmaps {
            buf.write_varint(*ty);
            buf.write_varint(data.len() as i32);
            data.iter().for_each(|&x| buf.write_long(x));
        }

        let mut data = Vec::new();
        for chunk_section in &self.chunk_sections {
            data.write_short(chunk_section.block_count);
            let containers = [&chunk_section.block_states, &chunk_section.biomes];
            for container in containers {
                data.write_unsigned_byte(container.bits_per_entry);

                // Palette
                if container.bits_per_entry == 0 {
                    // Single valued palette
                    let palette = container
                        .palette
                        .as_ref()
                        .expect("container with 0 bits per entry should have palette");
                    let item = *palette.first().expect(
                        "container with 0 bits per entry should have palette with one entry",
                    );
                    data.write_varint(item);
                } else if let Some(palette) = &container.palette {
                    // Indirect palette
                    data.write_varint(palette.len() as i32);
                    for palette_entry in palette {
                        data.write_varint(*palette_entry);
                    }
                }

                // Data Array
                for long in &container.data_array {
                    data.write_long(*long as i64);
                }
            }
        }
        buf.write_varint(data.len() as i32);
        buf.write_bytes(&data);
        // Number of block entities
        buf.write_varint(self.block_entities.len() as i32);
        for block_entity in &self.block_entities {
            buf.write_byte((block_entity.x << 4) | block_entity.z);
            buf.write_short(block_entity.y);
            buf.write_varint(block_entity.ty);
            buf.write_nbt(&block_entity.data);
        }

        // We don't do lighting because we have max ambient light
        // These will all be zeros

        // Sky Light Mask
        buf.write_varint(0);
        // Block Light Mask
        buf.write_varint(0);

        let bits = bits![u64, Lsb0; 1].repeat(self.chunk_sections.len() + 2);
        let longs = bits.as_raw_slice();
        // Empty Sky Light Mask
        buf.write_varint(longs.len() as i32);
        longs.iter().for_each(|&x| buf.write_long(x as i64));
        // Empty Block Light Mask
        buf.write_varint(longs.len() as i32);
        longs.iter().for_each(|&x| buf.write_long(x as i64));

        // Sky Light array count
        buf.write_varint(0);
        // Block Light array count
        buf.write_varint(0);

        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:level_chunk_with_light"),
        )
    }
}

pub struct CWorldEvent {
    pub event: i32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub data: i32,
    pub disable_relative_volume: bool,
}

impl ClientBoundPacket for CWorldEvent {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_int(self.event);
        buf.write_position(self.x, self.y, self.z);
        buf.write_int(self.data);
        buf.write_bool(self.disable_relative_volume);
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:level_event"),
        )
    }
}

pub struct CLoginDeathLocation {
    dimension_name: String,
    x: i32,
    y: i32,
    z: i32,
}

pub struct CLogin {
    pub entity_id: i32,
    pub is_hardcore: bool,
    pub dimension_names: Vec<String>,
    pub max_players: i32,
    pub view_distance: i32,
    pub simulation_distance: i32,
    pub reduced_debug_info: bool,
    pub enable_respawn_screen: bool,
    pub do_limited_crafting: bool,
    pub dimension_type: i32,
    pub dimension_name: String,
    pub hashed_seed: u64,
    pub gamemode: u8,
    pub previous_gamemode: i8,
    pub is_debug: bool,
    pub is_flat: bool,
    pub death_location: Option<CLoginDeathLocation>,
    pub portal_cooldown: i32,
    pub sea_level: i32,
    pub enforces_secure_chat: bool,
}

impl ClientBoundPacket for CLogin {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_int(self.entity_id);
        buf.write_bool(self.is_hardcore);
        buf.write_varint(self.dimension_names.len() as i32);
        for name in &self.dimension_names {
            buf.write_identifier(name);
        }
        buf.write_varint(self.max_players);
        buf.write_varint(self.view_distance);
        buf.write_varint(self.simulation_distance);
        buf.write_bool(self.reduced_debug_info);
        buf.write_bool(self.enable_respawn_screen);
        buf.write_bool(self.do_limited_crafting);
        buf.write_varint(self.dimension_type);
        buf.write_identifier(&self.dimension_name);
        buf.write_long(self.hashed_seed as i64);
        buf.write_unsigned_byte(self.gamemode);
        buf.write_byte(self.previous_gamemode);
        buf.write_bool(self.is_debug);
        buf.write_bool(self.is_flat);
        buf.write_bool(self.death_location.is_some());
        if let Some(death_location) = &self.death_location {
            buf.write_identifier(&death_location.dimension_name);
            buf.write_position(death_location.x, death_location.y, death_location.z);
        }
        buf.write_varint(self.portal_cooldown);
        buf.write_varint(self.sea_level);
        buf.write_bool(self.enforces_secure_chat);
        PacketEncoder::new(buf, packet_id!("play", "clientbound", "minecraft:login"))
    }
}

pub struct COpenSignEditor {
    pub pos_x: i32,
    pub pos_y: i32,
    pub pos_z: i32,
    pub is_front_text: bool,
}

impl ClientBoundPacket for COpenSignEditor {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_position(self.pos_x, self.pos_y, self.pos_z);
        buf.write_bool(self.is_front_text);
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:open_sign_editor"),
        )
    }
}

pub struct CUpdateEntityPosition {
    pub entity_id: i32,
    pub delta_x: i16,
    pub delta_y: i16,
    pub delta_z: i16,
    pub on_ground: bool,
}

impl ClientBoundPacket for CUpdateEntityPosition {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.entity_id);
        buf.write_short(self.delta_x);
        buf.write_short(self.delta_y);
        buf.write_short(self.delta_z);
        buf.write_bool(self.on_ground);
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:move_entity_pos"),
        )
    }
}

pub struct CUpdateEntityPositionAndRotation {
    pub entity_id: i32,
    pub delta_x: i16,
    pub delta_y: i16,
    pub delta_z: i16,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
}

impl ClientBoundPacket for CUpdateEntityPositionAndRotation {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.entity_id);
        buf.write_short(self.delta_x);
        buf.write_short(self.delta_y);
        buf.write_short(self.delta_z);
        buf.write_byte(((self.yaw / 360f32 * 256f32) as i32 % 256) as i8);
        buf.write_byte(((self.pitch / 360f32 * 256f32) as i32 % 256) as i8);
        buf.write_bool(self.on_ground);
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:move_entity_pos_rot"),
        )
    }
}

pub struct CEntityRotation {
    pub entity_id: i32,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
}

impl ClientBoundPacket for CEntityRotation {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.entity_id);
        buf.write_byte(((self.yaw / 360f32 * 256f32) as i32 % 256) as i8);
        buf.write_byte(((self.pitch / 360f32 * 256f32) as i32 % 256) as i8);
        buf.write_bool(self.on_ground);
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:move_entity_rot"),
        )
    }
}

pub struct COpenScreen {
    pub window_id: i32,
    pub window_type: i32,
    pub window_title: TextComponent,
}

impl ClientBoundPacket for COpenScreen {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.window_id);
        buf.write_varint(self.window_type);
        buf.write_text_component(&self.window_title);
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:open_screen"),
        )
    }
}

pub struct CPlayerAbilities {
    pub flags: u8,
    pub fly_speed: f32,
    pub fov_modifier: f32,
}

impl ClientBoundPacket for CPlayerAbilities {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_unsigned_byte(self.flags);
        buf.write_float(self.fly_speed);
        buf.write_float(self.fov_modifier);
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:player_abilities"),
        )
    }
}

pub struct CPlayerInfoRemove {
    pub players: Vec<u128>,
}

impl ClientBoundPacket for CPlayerInfoRemove {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.players.len() as i32);
        for &uuid in &self.players {
            buf.write_uuid(uuid);
        }
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:player_info_remove"),
        )
    }
}

pub struct CPlayerInfoAddPlayer {
    pub name: String,
    pub properties: Vec<PlayerProperty>,
}

#[derive(Default)]
pub struct CPlayerInfoActions {
    pub add_player: Option<CPlayerInfoAddPlayer>,
    pub update_gamemode: Option<i32>,
    pub update_listed: Option<bool>,
    pub update_latency: Option<i32>,
    pub update_display_name: Option<Option<TextComponent>>,
}

impl CPlayerInfoActions {
    fn get_mask(&self) -> u8 {
        let mut mask = 0;

        if self.add_player.is_some() {
            mask |= 0x01;
        }
        if self.update_gamemode.is_some() {
            mask |= 0x04;
        }
        if self.update_listed.is_some() {
            mask |= 0x08;
        }
        if self.update_latency.is_some() {
            mask |= 0x10;
        }
        if self.update_display_name.is_some() {
            mask |= 0x20;
        }

        mask
    }

    fn encode(&self, buf: &mut Vec<u8>) {
        if let Some(add_player) = &self.add_player {
            buf.write_string(16, &add_player.name);
            buf.write_varint(add_player.properties.len() as i32);
            for prop in &add_player.properties {
                buf.write_player_property(prop);
            }
        }
        if let Some(gamemode) = self.update_gamemode {
            buf.write_varint(gamemode);
        }
        if let Some(listed) = self.update_listed {
            buf.write_bool(listed);
        }
        if let Some(latency) = self.update_latency {
            buf.write_varint(latency);
        }
        if let Some(display_name) = &self.update_display_name {
            buf.write_bool(display_name.is_some());
            if let Some(display_name) = display_name {
                buf.write_text_component(display_name);
            }
        }
    }
}

pub struct CPlayerInfoUpdatePlayer {
    pub uuid: u128,
    pub actions: CPlayerInfoActions,
}

pub struct CPlayerInfoUpdate {
    // All player actions must have the same fields filled out
    pub players: Vec<CPlayerInfoUpdatePlayer>,
}

impl ClientBoundPacket for CPlayerInfoUpdate {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        let mask = self
            .players
            .first()
            .map(|player| player.actions.get_mask())
            .unwrap_or(0);
        buf.write_unsigned_byte(mask);
        buf.write_varint(self.players.len() as i32);
        for player in &self.players {
            buf.write_uuid(player.uuid);
            player.actions.encode(&mut buf);
        }
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:player_info_update"),
        )
    }
}

pub struct CSynchronizePlayerPosition {
    pub teleport_id: i32,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub velocity_x: f64,
    pub velocity_y: f64,
    pub velocity_z: f64,
    pub yaw: f32,
    pub pitch: f32,
    pub flags: i32,
}

impl ClientBoundPacket for CSynchronizePlayerPosition {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.teleport_id);
        buf.write_double(self.x);
        buf.write_double(self.y);
        buf.write_double(self.z);
        buf.write_double(self.velocity_x);
        buf.write_double(self.velocity_y);
        buf.write_double(self.velocity_z);
        buf.write_float(self.yaw);
        buf.write_float(self.pitch);
        buf.write_int(self.flags);
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:player_position"),
        )
    }
}

pub struct CRemoveEntities {
    pub entity_ids: Vec<i32>,
}

impl ClientBoundPacket for CRemoveEntities {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.entity_ids.len() as i32);
        for &entity_id in &self.entity_ids {
            buf.write_varint(entity_id);
        }
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:remove_entities"),
        )
    }
}

pub struct CResetScore {
    pub entity_name: String,
    pub objective_name: Option<String>,
}

impl ClientBoundPacket for CResetScore {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_string(32767, &self.entity_name);
        buf.write_bool(self.objective_name.is_some());
        if let Some(objective_name) = &self.objective_name {
            buf.write_string(32767, objective_name);
        }
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:reset_score"),
        )
    }
}

pub struct CSetHeadRotation {
    pub entity_id: i32,
    pub head_yaw: f32,
}

impl ClientBoundPacket for CSetHeadRotation {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.entity_id);
        buf.write_byte(((self.head_yaw / 360f32 * 256f32) as i32 % 256) as i8);
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:rotate_head"),
        )
    }
}

#[derive(Debug, Clone)]
pub struct CUpdateSectionBlocksRecord {
    pub x: u8,
    pub y: u8,
    pub z: u8,
    pub block_id: u32,
}

#[derive(Debug, Clone)]
pub struct CUpdateSectionBlocks {
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub chunk_y: u32,
    pub records: Vec<CUpdateSectionBlocksRecord>,
}

impl ClientBoundPacket for CUpdateSectionBlocks {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::with_capacity(self.records.len() * 8 + 12);
        let pos = ((self.chunk_x as i64 & 0x3FFFFF) << 42)
            | ((self.chunk_z as i64 & 0x3FFFFF) << 20)
            | (self.chunk_y as i64 & 0xFFFFF);
        buf.write_long(pos);
        buf.write_varint(self.records.len() as i32); // Length of record array
        for record in &self.records {
            let long = ((record.block_id as u64) << 12)
                | ((record.x as u64) << 8)
                | ((record.z as u64) << 4)
                | (record.y as u64);
            buf.write_varlong(long as i64);
        }

        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:section_blocks_update"),
        )
    }
}

pub struct CSetHeldItem {
    pub slot: i8,
}

impl ClientBoundPacket for CSetHeldItem {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_byte(self.slot);
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:set_held_slot"),
        )
    }
}

pub struct CSetCenterChunk {
    pub chunk_x: i32,
    pub chunk_z: i32,
}

impl ClientBoundPacket for CSetCenterChunk {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.chunk_x);
        buf.write_varint(self.chunk_z);
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:set_chunk_cache_center"),
        )
    }
}

pub struct CDisplayObjective {
    pub position: u8,
    pub score_name: String,
}

impl ClientBoundPacket for CDisplayObjective {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_byte(self.position as i8);
        buf.write_string(32767, &self.score_name);
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:set_display_objective"),
        )
    }
}

pub struct CSetEntityMetadataEntry {
    pub index: u8,
    pub metadata_type: i32,
    pub value: Vec<u8>,
}

pub struct CSetEntityMetadata {
    pub entity_id: i32,
    pub metadata: Vec<CSetEntityMetadataEntry>,
}

impl ClientBoundPacket for CSetEntityMetadata {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.entity_id);
        for entry in &self.metadata {
            buf.write_unsigned_byte(entry.index);
            buf.write_varint(entry.metadata_type);
            buf.write_bytes(&entry.value);
        }
        buf.write_byte(-1); // 0xFF
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:set_entity_data"),
        )
    }
}

pub struct CSetEquipmentEquipment {
    pub slot: i8,
    pub item: Option<SlotData>,
}

pub struct CSetEquipment {
    pub entity_id: i32,
    pub equipment: Vec<CSetEquipmentEquipment>,
}

impl ClientBoundPacket for CSetEquipment {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.entity_id);
        for slot in &self.equipment {
            buf.write_byte(slot.slot);
            buf.write_slot_data(&slot.item);
        }

        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:set_equipment"),
        )
    }
}

pub enum ObjectiveNumberFormat {
    Blank,
    Styled { styling: NBTCompound },
    Fixed { content: TextComponent },
}

impl ObjectiveNumberFormat {
    fn write_to_buf(&self, buf: &mut Vec<u8>) {
        match self {
            ObjectiveNumberFormat::Blank => {
                buf.write_varint(0);
            }
            ObjectiveNumberFormat::Styled { styling } => {
                buf.write_varint(1);
                buf.write_nbt(styling);
            }
            ObjectiveNumberFormat::Fixed { content } => {
                buf.write_varint(2);
                buf.write_text_component(content);
            }
        };
    }
}

pub struct CUpdateScore {
    pub entity_name: String,
    pub objective_name: String,
    pub value: i32,
    pub display_name: Option<TextComponent>,
    pub number_format: Option<ObjectiveNumberFormat>,
}

impl ClientBoundPacket for CUpdateScore {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_string(32767, &self.entity_name);
        buf.write_string(32767, &self.objective_name);
        buf.write_varint(self.value);
        buf.write_bool(self.display_name.is_some());
        if let Some(display_name) = &self.display_name {
            buf.write_text_component(display_name);
        }
        buf.write_bool(self.number_format.is_some());
        if let Some(number_format) = &self.number_format {
            number_format.write_to_buf(&mut buf);
        }
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:set_score"),
        )
    }
}

pub struct CUpdateObjectivesObjective {
    pub value: TextComponent,
    pub ty: u32,
    pub number_format: Option<ObjectiveNumberFormat>,
}

pub struct CUpdateObjectives {
    pub objective_name: String,
    pub mode: u8,
    pub objective: Option<CUpdateObjectivesObjective>,
}

impl ClientBoundPacket for CUpdateObjectives {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_string(16, &self.objective_name);
        buf.write_byte(self.mode as i8);
        if let Some(objective) = &self.objective
            && (self.mode == 0 || self.mode == 2)
        {
            buf.write_text_component(&objective.value);
            buf.write_varint(objective.ty as i32);
            if let Some(number_format) = &objective.number_format {
                number_format.write_to_buf(&mut buf);
            }
        }
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:set_objective"),
        )
    }
}

pub struct UpdateTime {
    pub world_age: i64,
    pub time_of_day: i64,
    pub time_of_day_increasing: bool,
}

impl ClientBoundPacket for UpdateTime {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_long(self.world_age);
        buf.write_long(self.time_of_day);
        buf.write_bool(self.time_of_day_increasing);
        PacketEncoder::new(buf, packet_id!("play", "clientbound", "minecraft:set_time"))
    }
}

pub struct CSoundEffect {
    pub sound_id: i32,
    pub sound_name: Option<String>,
    pub has_fixed_range: Option<bool>,
    pub range: Option<bool>,
    pub sound_category: i32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub volume: f32,
    pub pitch: f32,
    pub seed: i64,
}

impl ClientBoundPacket for CSoundEffect {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.sound_id);
        buf.write_varint(self.sound_category);
        buf.write_int(self.x);
        buf.write_int(self.y);
        buf.write_int(self.z);
        buf.write_float(self.volume);
        buf.write_float(self.pitch);
        buf.write_long(self.seed);
        PacketEncoder::new(buf, packet_id!("play", "clientbound", "minecraft:sound"))
    }
}

pub struct CTeleportEntity {
    pub entity_id: i32,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
}

impl ClientBoundPacket for CTeleportEntity {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_varint(self.entity_id);
        buf.write_double(self.x);
        buf.write_double(self.y);
        buf.write_double(self.z);
        buf.write_byte(((self.yaw / 360f32 * 256f32) as i32 % 256) as i8);
        buf.write_byte(((self.pitch / 360f32 * 256f32) as i32 % 256) as i8);
        buf.write_bool(self.on_ground);
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:entity_position_sync"),
        )
    }
}

pub struct CSystemChatMessage {
    pub content: TextComponent,
    pub overlay: bool,
}

impl ClientBoundPacket for CSystemChatMessage {
    fn encode(&self) -> PacketEncoder {
        let mut buf = Vec::new();
        buf.write_text_component(&self.content);
        buf.write_bool(self.overlay);
        PacketEncoder::new(
            buf,
            packet_id!("play", "clientbound", "minecraft:system_chat"),
        )
    }
}
