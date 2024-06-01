use super::{DecodeResult, PacketDecoderExt, SlotData};

pub trait ServerBoundPacketHandler {
    // Handshaking
    fn handle_handshake(&mut self, _packet: SHandshake, _player_idx: usize) {}
    // Status
    fn handle_request(&mut self, _packet: SRequest, _player_idx: usize) {}
    fn handle_ping(&mut self, _packet: SPing, _player_idx: usize) {}
    // Login
    fn handle_login_start(&mut self, _packet: SLoginStart, _player_idx: usize) {}
    fn handle_login_acknowledged(&mut self, _packet: SLoginAcknowledged, _player_idx: usize) {}
    // Configuration
    fn handle_client_information(&mut self, _packet: SClientInformation, _player_idx: usize) {}
    fn handle_acknowledge_finish_configuration(
        &mut self,
        _packet: SAcknowledgeFinishConfiguration,
        _player_idx: usize,
    ) {
    }
    // Play
    fn handle_chat_message(&mut self, _packet: SChatMessage, _player_idx: usize) {}
    fn handle_command_suggestions_request(
        &mut self,
        _packet: SCommandSuggestionsRequest,
        _player_idx: usize,
    ) {
    }
    fn handle_plugin_message(&mut self, _packet: SPluginMessage, _player_idx: usize) {}
    fn handle_keep_alive(&mut self, _packet: SKeepAlive, _player_idx: usize) {}
    fn handle_set_player_position(&mut self, _packet: SSetPlayerPosition, _player_idx: usize) {}
    fn handle_set_player_position_and_rotation(
        &mut self,
        _packet: SSetPlayerPositionAndRotation,
        _player_idx: usize,
    ) {
    }
    fn handle_player_rotation(&mut self, _packet: SPlayerRotation, _player_idx: usize) {}
    fn handle_set_player_on_ground(&mut self, _packet: SSetPlayerOnGround, _player_idx: usize) {}
    fn handle_player_abilities(&mut self, _packet: SPlayerAbilities, _player_idx: usize) {}
    fn handle_player_action(&mut self, _packet: SPlayerAction, _player_idx: usize) {}
    fn handle_player_command(&mut self, _packet: SPlayerCommand, _player_idx: usize) {}
    fn handle_swing_arm(&mut self, _packet: SSwingArm, _player_idx: usize) {}
    fn handle_use_item_on(&mut self, _packet: SUseItemOn, _player_idx: usize) {}
    fn handle_set_held_item(&mut self, _packet: SSetHeldItem, _player_idx: usize) {}
    fn handle_set_creative_mode_slot(&mut self, _packet: SSetCreativeModeSlot, _player_idx: usize) {
    }
    fn handle_update_sign(&mut self, _packet: SUpdateSign, _player_idx: usize) {}
    fn handle_unknown(&mut self, _packet: SUnknown, _player_idx: usize) {}
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

pub struct SHandshake {
    pub protocol_version: i32,
    pub server_address: String,
    pub server_port: u16,
    pub next_state: i32,
}

impl ServerBoundPacket for SHandshake {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(SHandshake {
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

pub struct SRequest;

impl ServerBoundPacket for SRequest {
    fn decode<T: PacketDecoderExt>(_decoder: &mut T) -> DecodeResult<Self> {
        Ok(SRequest)
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_request(*self, player_idx);
    }
}

pub struct SPing {
    pub payload: i64,
}

impl ServerBoundPacket for SPing {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(SPing {
            payload: decoder.read_long()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_ping(*self, player_idx);
    }
}

pub struct SLoginStart {
    pub name: String,
}

impl ServerBoundPacket for SLoginStart {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(SLoginStart {
            name: decoder.read_string()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_login_start(*self, player_idx);
    }
}

pub struct SLoginAcknowledged;

impl ServerBoundPacket for SLoginAcknowledged {
    fn decode<T: PacketDecoderExt>(_decoder: &mut T) -> DecodeResult<Self> {
        Ok(SLoginAcknowledged)
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_login_acknowledged(*self, player_idx);
    }
}

pub struct SClientInformation {
    pub locale: String,
    pub view_distance: i8,
    pub chat_mode: i32,
    pub chat_colors: bool,
    pub displayed_skin_parts: u8,
    pub main_hand: i32,
    pub enable_text_filtering: bool,
    pub allow_server_listings: bool,
}

impl ServerBoundPacket for SClientInformation {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(SClientInformation {
            locale: decoder.read_string()?,
            view_distance: decoder.read_byte()?,
            chat_mode: decoder.read_varint()?,
            chat_colors: decoder.read_bool()?,
            displayed_skin_parts: decoder.read_unsigned_byte()?,
            main_hand: decoder.read_varint()?,
            enable_text_filtering: decoder.read_bool()?,
            allow_server_listings: decoder.read_bool()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_client_information(*self, player_idx);
    }
}

pub struct SAcknowledgeFinishConfiguration;

impl ServerBoundPacket for SAcknowledgeFinishConfiguration {
    fn decode<T: PacketDecoderExt>(_decoder: &mut T) -> DecodeResult<Self> {
        Ok(SAcknowledgeFinishConfiguration)
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_acknowledge_finish_configuration(*self, player_idx);
    }
}

pub struct SChatMessage {
    pub message: String,
    pub timestamp: i64,
    pub salt: i64,
    pub signature: Option<Vec<u8>>,
    pub message_count: i32,
    pub acknowledged: [u8; 3],
}

impl ServerBoundPacket for SChatMessage {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        let message = decoder.read_string()?;
        let timestamp = decoder.read_long()?;
        let salt = decoder.read_long()?;
        let has_signature = decoder.read_bool()?;
        let signature = if has_signature {
            Some(decoder.read_bytes(256)?)
        } else {
            None
        };
        let message_count = decoder.read_varint()?;
        let acknowledged = decoder.read_bytes(3)?.try_into().unwrap();
        Ok(SChatMessage {
            message,
            timestamp,
            salt,
            signature,
            message_count,
            acknowledged,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_chat_message(*self, player_idx);
    }
}

pub struct SCommandSuggestionsRequest {
    pub transaction_id: i32,
    pub text: String,
}

impl ServerBoundPacket for SCommandSuggestionsRequest {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(SCommandSuggestionsRequest {
            transaction_id: decoder.read_varint()?,
            text: decoder.read_string()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_command_suggestions_request(*self, player_idx);
    }
}

pub struct SPluginMessage {
    pub channel: String,
    pub data: Vec<u8>,
}

impl ServerBoundPacket for SPluginMessage {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(SPluginMessage {
            channel: decoder.read_string()?,
            data: PacketDecoderExt::read_to_end(decoder)?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_plugin_message(*self, player_idx);
    }
}

pub struct SKeepAlive {
    pub id: i64,
}

impl ServerBoundPacket for SKeepAlive {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(SKeepAlive {
            id: decoder.read_long()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_keep_alive(*self, player_idx);
    }
}

pub struct SSetPlayerPosition {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub on_ground: bool,
}

impl ServerBoundPacket for SSetPlayerPosition {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(SSetPlayerPosition {
            x: decoder.read_double()?,
            y: decoder.read_double()?,
            z: decoder.read_double()?,
            on_ground: decoder.read_bool()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_set_player_position(*self, player_idx);
    }
}

pub struct SSetPlayerPositionAndRotation {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
}

impl ServerBoundPacket for SSetPlayerPositionAndRotation {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(SSetPlayerPositionAndRotation {
            x: decoder.read_double()?,
            y: decoder.read_double()?,
            z: decoder.read_double()?,
            yaw: decoder.read_float()?,
            pitch: decoder.read_float()?,
            on_ground: decoder.read_bool()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_set_player_position_and_rotation(*self, player_idx);
    }
}

pub struct SPlayerRotation {
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
}

impl ServerBoundPacket for SPlayerRotation {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(SPlayerRotation {
            yaw: decoder.read_float()?,
            pitch: decoder.read_float()?,
            on_ground: decoder.read_bool()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_player_rotation(*self, player_idx);
    }
}

pub struct SSetPlayerOnGround {
    pub on_ground: bool,
}

impl ServerBoundPacket for SSetPlayerOnGround {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(SSetPlayerOnGround {
            on_ground: decoder.read_bool()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_set_player_on_ground(*self, player_idx);
    }
}

pub struct SPlayerAbilities {
    pub is_flying: bool,
}

impl ServerBoundPacket for SPlayerAbilities {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(SPlayerAbilities {
            is_flying: decoder.read_byte()? != 0,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_player_abilities(*self, player_idx);
    }
}

pub struct SPlayerAction {
    pub status: i32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub face: i8,
    pub sequence: i32,
}

impl ServerBoundPacket for SPlayerAction {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        let status = decoder.read_varint()?;
        let location = decoder.read_position()?;
        let face = decoder.read_byte()?;
        let sequence = decoder.read_varint()?;
        Ok(SPlayerAction {
            x: location.0,
            y: location.1,
            z: location.2,
            status,
            face,
            sequence,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_player_action(*self, player_idx);
    }
}

pub struct SPlayerCommand {
    pub entity_id: i32,
    pub action_id: i32,
    pub jump_boost: i32,
}

impl ServerBoundPacket for SPlayerCommand {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(SPlayerCommand {
            entity_id: decoder.read_varint()?,
            action_id: decoder.read_varint()?,
            jump_boost: decoder.read_varint()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_player_command(*self, player_idx);
    }
}

pub struct SSwingArm {
    pub hand: i32,
}

impl ServerBoundPacket for SSwingArm {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(SSwingArm {
            hand: decoder.read_varint()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_swing_arm(*self, player_idx);
    }
}

pub struct SUseItemOn {
    pub hand: i32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub face: i32,
    pub cursor_x: f32,
    pub cursor_y: f32,
    pub cursor_z: f32,
    pub inside_block: bool,
    pub sequence: i32,
}

impl ServerBoundPacket for SUseItemOn {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        let hand = decoder.read_varint()?;
        let location = decoder.read_position()?;
        let face = decoder.read_varint()?;
        let cursor_x = decoder.read_float()?;
        let cursor_y = decoder.read_float()?;
        let cursor_z = decoder.read_float()?;
        let inside_block = decoder.read_bool()?;
        let sequence = decoder.read_varint()?;
        Ok(SUseItemOn {
            x: location.0,
            y: location.1,
            z: location.2,
            hand,
            face,
            cursor_x,
            cursor_y,
            cursor_z,
            inside_block,
            sequence,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_use_item_on(*self, player_idx);
    }
}

pub struct SSetHeldItem {
    pub slot: i16,
}

impl ServerBoundPacket for SSetHeldItem {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        Ok(SSetHeldItem {
            slot: decoder.read_short()?,
        })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_set_held_item(*self, player_idx);
    }
}

pub struct SSetCreativeModeSlot {
    pub slot: i16,
    pub clicked_item: Option<SlotData>,
}

impl ServerBoundPacket for SSetCreativeModeSlot {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        let slot = decoder.read_short()?;
        let clicked_item = if decoder.read_bool()? {
            Some(SlotData {
                item_id: decoder.read_varint()?,
                item_count: decoder.read_byte()?,
                nbt: decoder.read_nbt_compound()?,
            })
        } else {
            None
        };
        Ok(SSetCreativeModeSlot { slot, clicked_item })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_set_creative_mode_slot(*self, player_idx);
    }
}

pub struct SUpdateSign {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub lines: [String; 4],
}

impl ServerBoundPacket for SUpdateSign {
    fn decode<T: PacketDecoderExt>(decoder: &mut T) -> DecodeResult<Self> {
        let (x, y, z) = decoder.read_position()?;
        let lines = [
            decoder.read_string()?,
            decoder.read_string()?,
            decoder.read_string()?,
            decoder.read_string()?,
        ];
        Ok(SUpdateSign { x, y, z, lines })
    }

    fn handle(self: Box<Self>, handler: &mut dyn ServerBoundPacketHandler, player_idx: usize) {
        handler.handle_update_sign(*self, player_idx);
    }
}
