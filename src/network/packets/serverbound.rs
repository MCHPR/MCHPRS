use super::{DecodeResult, PacketDecoderExt, SlotData};

pub trait ServerBoundPacketHandler {
    fn handle_handshake(&mut self, _packet: S00Handshake, _player_idx: usize) {}
    fn handle_request(&mut self, _packet: S00Request, _player_idx: usize) {}
    fn handle_ping(&mut self, _packet: S01Ping, _player_idx: usize) {}
    fn handle_login_start(&mut self, _packet: S00LoginStart, _player_idx: usize) {}
    fn handle_chat_message(&mut self, _packet: S03ChatMessage, _player_idx: usize) {}
    fn handle_client_settings(&mut self, _packet: S05ClientSettings, _player_idx: usize) {}
    fn handle_plugin_message(&mut self, _packet: S0BPluginMessage, _player_idx: usize) {}
    fn handle_keep_alive(&mut self, _packet: S10KeepAlive, _player_idx: usize) {}
    fn handle_player_position(&mut self, _packet: S12PlayerPosition, _player_idx: usize) {}
    fn handle_player_position_and_rotation(
        &mut self,
        _packet: S13PlayerPositionAndRotation,
        _player_idx: usize,
    ) {
    }
    fn handle_player_rotation(&mut self, _packet: S14PlayerRotation, _player_idx: usize) {}
    fn handle_player_movement(&mut self, _packet: S15PlayerMovement, _player_idx: usize) {}
    fn handle_player_abilities(&mut self, _packet: S1APlayerAbilities, _player_idx: usize) {}
    fn handle_player_digging(&mut self, _packet: S1BPlayerDigging, _player_idx: usize) {}
    fn handle_entity_action(&mut self, _packet: S1CEntityAction, _player_idx: usize) {}
    fn handle_animation(&mut self, _packet: S2CAnimation, _player_idx: usize) {}
    fn handle_player_block_placement(
        &mut self,
        _packet: S2EPlayerBlockPlacemnt,
        _player_idx: usize,
    ) {
    }
    fn handle_held_item_change(&mut self, _packet: S25HeldItemChange, _player_idx: usize) {}
    fn handle_creative_inventory_action(
        &mut self,
        _packet: S28CreativeInventoryAction,
        _player_idx: usize,
    ) {
    }
    fn handle_unknown(&mut self, _packet: SUnknown, __player_idx: usize) {}
}

pub trait ServerBoundPacket: Send {
    fn decode<T: PacketDecoderExt>(reader: &mut T) -> DecodeResult<Self>
    where
        Self: Sized;

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize);
}

pub struct SUnknown;

impl ServerBoundPacket for SUnknown {
    fn decode<T: PacketDecoderExt>(_: &mut T) -> DecodeResult<Self> {
        Ok(SUnknown)
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_unknown(*self, player_idx);
    }
}

pub struct S00Handshake {
    pub protocol_version: i32,
    pub server_address: String,
    pub server_port: u16,
    pub next_state: i32,
}

impl ServerBoundPacket for S00Handshake {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(S00Handshake {
            protocol_version: decoder.read_varint()?,
            server_address: decoder.read_string()?,
            server_port: decoder.read_unsigned_short()?,
            next_state: decoder.read_varint()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_handshake(*self, player_idx);
    }
}

pub struct S00Request;

impl ServerBoundPacket for S00Request {
    fn decode<T: PacketDecoderExt>(_decoder: &mut T) -> DecodeResult<Self> {
        Ok(S00Request)
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_request(*self, player_idx);
    }
}

pub struct S01Ping {
    pub payload: i64,
}

impl ServerBoundPacket for S01Ping {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(S01Ping {
            payload: decoder.read_long()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_ping(*self, player_idx);
    }
}

pub struct S00LoginStart {
    pub name: String,
}

impl ServerBoundPacket for S00LoginStart {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(S00LoginStart {
            name: decoder.read_string()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_login_start(*self, player_idx);
    }
}

pub struct S03ChatMessage {
    pub message: String,
}

impl ServerBoundPacket for S03ChatMessage {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(S03ChatMessage {
            message: decoder.read_string()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_chat_message(*self, player_idx);
    }
}

pub struct S05ClientSettings {
    pub locale: String,
    pub view_distance: i8,
    pub chat_mode: i32,
    pub chat_colors: bool,
    pub displayed_skin_parts: u8,
    pub main_hand: i32,
}

impl ServerBoundPacket for S05ClientSettings {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(S05ClientSettings {
            locale: decoder.read_string()?,
            view_distance: decoder.read_byte()?,
            chat_mode: decoder.read_varint()?,
            chat_colors: decoder.read_bool()?,
            displayed_skin_parts: decoder.read_unsigned_byte()?,
            main_hand: decoder.read_varint()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_client_settings(*self, player_idx);
    }
}

pub struct S0BPluginMessage {
    pub channel: String,
    pub data: Vec<u8>,
}

impl ServerBoundPacket for S0BPluginMessage {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(S0BPluginMessage {
            channel: decoder.read_string()?,
            data: PacketDecoderExt::read_to_end(decoder)?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_plugin_message(*self, player_idx);
    }
}

pub struct S10KeepAlive {
    pub id: i64,
}

impl ServerBoundPacket for S10KeepAlive {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(S10KeepAlive {
            id: decoder.read_long()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_keep_alive(*self, player_idx);
    }
}

pub struct S12PlayerPosition {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub on_ground: bool,
}

impl ServerBoundPacket for S12PlayerPosition {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(S12PlayerPosition {
            x: decoder.read_double()?,
            y: decoder.read_double()?,
            z: decoder.read_double()?,
            on_ground: decoder.read_bool()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_player_position(*self, player_idx);
    }
}

pub struct S13PlayerPositionAndRotation {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
}

impl ServerBoundPacket for S13PlayerPositionAndRotation {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(S13PlayerPositionAndRotation {
            x: decoder.read_double()?,
            y: decoder.read_double()?,
            z: decoder.read_double()?,
            yaw: decoder.read_float()?,
            pitch: decoder.read_float()?,
            on_ground: decoder.read_bool()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_player_position_and_rotation(*self, player_idx);
    }
}

pub struct S14PlayerRotation {
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
}

impl ServerBoundPacket for S14PlayerRotation {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(S14PlayerRotation {
            yaw: decoder.read_float()?,
            pitch: decoder.read_float()?,
            on_ground: decoder.read_bool()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_player_rotation(*self, player_idx);
    }
}

pub struct S15PlayerMovement {
    pub on_ground: bool,
}

impl ServerBoundPacket for S15PlayerMovement {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(S15PlayerMovement {
            on_ground: decoder.read_bool()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_player_movement(*self, player_idx);
    }
}

pub struct S1APlayerAbilities {
    pub is_flying: bool,
}

impl ServerBoundPacket for S1APlayerAbilities {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(S1APlayerAbilities {
            is_flying: decoder.read_byte()? != 0,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_player_abilities(*self, player_idx);
    }
}

pub struct S1BPlayerDigging {
    pub status: i32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub face: i8,
}

impl ServerBoundPacket for S1BPlayerDigging {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        let status = decoder.read_varint()?;
        let location = decoder.read_position()?;
        let face = decoder.read_byte()?;
        Ok(S1BPlayerDigging {
            x: location.0,
            y: location.1,
            z: location.2,
            status,
            face,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_player_digging(*self, player_idx);
    }
}

pub struct S1CEntityAction {
    pub entity_id: i32,
    pub action_id: i32,
    pub jump_boost: i32,
}

impl ServerBoundPacket for S1CEntityAction {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(S1CEntityAction {
            entity_id: decoder.read_varint()?,
            action_id: decoder.read_varint()?,
            jump_boost: decoder.read_varint()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_entity_action(*self, player_idx);
    }
}

pub struct S2CAnimation {
    pub hand: i32,
}

impl ServerBoundPacket for S2CAnimation {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(S2CAnimation {
            hand: decoder.read_varint()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_animation(*self, player_idx);
    }
}

pub struct S2EPlayerBlockPlacemnt {
    pub hand: i32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub face: i32,
    pub cursor_x: f32,
    pub cursor_y: f32,
    pub cursor_z: f32,
    pub inside_block: bool,
}

impl ServerBoundPacket for S2EPlayerBlockPlacemnt {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        let hand = decoder.read_varint()?;
        let location = decoder.read_position()?;
        let face = decoder.read_varint()?;
        let cursor_x = decoder.read_float()?;
        let cursor_y = decoder.read_float()?;
        let cursor_z = decoder.read_float()?;
        let inside_block = decoder.read_bool()?;
        Ok(S2EPlayerBlockPlacemnt {
            x: location.0,
            y: location.1,
            z: location.2,
            hand,
            face,
            cursor_x,
            cursor_y,
            cursor_z,
            inside_block,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_player_block_placement(*self, player_idx);
    }
}

pub struct S25HeldItemChange {
    pub slot: i16,
}

impl ServerBoundPacket for S25HeldItemChange {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(S25HeldItemChange {
            slot: decoder.read_short()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_held_item_change(*self, player_idx);
    }
}

pub struct S28CreativeInventoryAction {
    pub slot: i16,
    pub clicked_item: Option<SlotData>,
}

impl ServerBoundPacket for S28CreativeInventoryAction {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        let slot = decoder.read_short()?;
        let clicked_item = if decoder.read_bool()? {
            Some(SlotData {
                item_id: decoder.read_varint()?,
                item_count: decoder.read_byte()?,
                nbt: decoder.read_nbt_blob()?,
            })
        } else {
            None
        };
        Ok(S28CreativeInventoryAction { slot, clicked_item })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_creative_inventory_action(*self, player_idx);
    }
}
