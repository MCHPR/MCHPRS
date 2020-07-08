use super::{DecodeResult, PacketDecoder, SlotData};

pub trait ServerBoundPacket {
    fn decode(decoder: PacketDecoder) -> DecodeResult<Self>
    where
        Self: Sized;
}

pub struct S00Handshake {
    pub protocol_version: i32,
    pub server_address: String,
    pub server_port: u16,
    pub next_state: i32,
}

impl ServerBoundPacket for S00Handshake {
    fn decode(mut decoder: PacketDecoder) -> DecodeResult<Self> {
        Ok(S00Handshake {
            protocol_version: decoder.read_varint()?,
            server_address: decoder.read_string()?,
            server_port: decoder.read_unsigned_short()?,
            next_state: decoder.read_varint()?,
        })
    }
}

pub struct S00Request {}

impl ServerBoundPacket for S00Request {
    fn decode(mut _decoder: PacketDecoder) -> DecodeResult<Self> {
        Ok(S00Request {})
    }
}

pub struct S00Ping {
    pub payload: i64,
}

impl ServerBoundPacket for S00Ping {
    fn decode(mut decoder: PacketDecoder) -> DecodeResult<Self> {
        Ok(S00Ping {
            payload: decoder.read_long()?,
        })
    }
}

pub struct S00LoginStart {
    pub name: String,
}

impl ServerBoundPacket for S00LoginStart {
    fn decode(mut decoder: PacketDecoder) -> DecodeResult<Self> {
        Ok(S00LoginStart {
            name: decoder.read_string()?,
        })
    }
}

pub struct S03ChatMessage {
    pub message: String,
}

impl ServerBoundPacket for S03ChatMessage {
    fn decode(mut decoder: PacketDecoder) -> DecodeResult<Self> {
        Ok(S03ChatMessage {
            message: decoder.read_string()?,
        })
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
    fn decode(mut decoder: PacketDecoder) -> DecodeResult<Self> {
        Ok(S05ClientSettings {
            locale: decoder.read_string()?,
            view_distance: decoder.read_byte()?,
            chat_mode: decoder.read_varint()?,
            chat_colors: decoder.read_bool()?,
            displayed_skin_parts: decoder.read_unsigned_byte()?,
            main_hand: decoder.read_varint()?,
        })
    }
}

pub struct S0BPluginMessage {
    pub channel: String,
    pub data: Vec<u8>,
}

impl ServerBoundPacket for S0BPluginMessage {
    fn decode(mut decoder: PacketDecoder) -> DecodeResult<Self> {
        Ok(S0BPluginMessage {
            channel: decoder.read_string()?,
            data: decoder.read_to_end()?,
        })
    }
}

pub struct S0FKeepAlive {
    pub id: i64,
}

impl ServerBoundPacket for S0FKeepAlive {
    fn decode(mut decoder: PacketDecoder) -> DecodeResult<Self> {
        Ok(S0FKeepAlive {
            id: decoder.read_long()?,
        })
    }
}

pub struct S11PlayerPosition {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub on_ground: bool,
}

impl ServerBoundPacket for S11PlayerPosition {
    fn decode(mut decoder: PacketDecoder) -> DecodeResult<Self> {
        Ok(S11PlayerPosition {
            x: decoder.read_double()?,
            y: decoder.read_double()?,
            z: decoder.read_double()?,
            on_ground: decoder.read_bool()?,
        })
    }
}

pub struct S12PlayerPositionAndRotation {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
}

impl ServerBoundPacket for S12PlayerPositionAndRotation {
    fn decode(mut decoder: PacketDecoder) -> DecodeResult<Self> {
        Ok(S12PlayerPositionAndRotation {
            x: decoder.read_double()?,
            y: decoder.read_double()?,
            z: decoder.read_double()?,
            yaw: decoder.read_float()?,
            pitch: decoder.read_float()?,
            on_ground: decoder.read_bool()?,
        })
    }
}

pub struct S13PlayerRotation {
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
}

impl ServerBoundPacket for S13PlayerRotation {
    fn decode(mut decoder: PacketDecoder) -> DecodeResult<Self> {
        Ok(S13PlayerRotation {
            yaw: decoder.read_float()?,
            pitch: decoder.read_float()?,
            on_ground: decoder.read_bool()?,
        })
    }
}

pub struct S14PlayerMovement {
    pub on_ground: bool,
}

impl ServerBoundPacket for S14PlayerMovement {
    fn decode(mut decoder: PacketDecoder) -> DecodeResult<Self> {
        Ok(S14PlayerMovement {
            on_ground: decoder.read_bool()?,
        })
    }
}

bitflags! {
    pub struct S19PlayerAbilitiesFlags: u32 {
        const GOD_MODE = 0x08;
        const CAN_FLY = 0x04;
        const IS_FLYING = 0x02;
        const CREATIVE = 0x01;
    }
}

pub struct S19PlayerAbilities {
    pub flags: S19PlayerAbilitiesFlags,
    pub flying_speed: f32,
    pub walking_speed: f32,
}

impl ServerBoundPacket for S19PlayerAbilities {
    fn decode(mut decoder: PacketDecoder) -> DecodeResult<Self> {
        Ok(S19PlayerAbilities {
            flags: S19PlayerAbilitiesFlags::from_bits_truncate(decoder.read_byte()? as u32),
            flying_speed: decoder.read_float()?,
            walking_speed: decoder.read_float()?,
        })
    }
}

pub struct S1APlayerDigging {
    pub status: i32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub face: i8,
}

impl ServerBoundPacket for S1APlayerDigging {
    fn decode(mut decoder: PacketDecoder) -> DecodeResult<Self> {
        let status = decoder.read_varint()?;
        let location = decoder.read_position()?;
        let face = decoder.read_byte()?;
        Ok(S1APlayerDigging {
            x: location.0,
            y: location.1,
            z: location.2,
            status,
            face,
        })
    }
}

pub struct S1BEntityAction {
    pub entity_id: i32,
    pub action_id: i32,
    pub jump_boost: i32,
}

impl ServerBoundPacket for S1BEntityAction {
    fn decode(mut decoder: PacketDecoder) -> DecodeResult<Self> {
        Ok(S1BEntityAction {
            entity_id: decoder.read_varint()?,
            action_id: decoder.read_varint()?,
            jump_boost: decoder.read_varint()?,
        })
    }
}

pub struct S2AAnimation {
    pub hand: i32,
}

impl ServerBoundPacket for S2AAnimation {
    fn decode(mut decoder: PacketDecoder) -> DecodeResult<Self> {
        Ok(S2AAnimation {
            hand: decoder.read_varint()?,
        })
    }
}

pub struct S2CPlayerBlockPlacemnt {
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

impl ServerBoundPacket for S2CPlayerBlockPlacemnt {
    fn decode(mut decoder: PacketDecoder) -> DecodeResult<Self> {
        let hand = decoder.read_varint()?;
        let location = decoder.read_position()?;
        let face = decoder.read_varint()?;
        let cursor_x = decoder.read_float()?;
        let cursor_y = decoder.read_float()?;
        let cursor_z = decoder.read_float()?;
        let inside_block = decoder.read_bool()?;
        Ok(S2CPlayerBlockPlacemnt {
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
}

pub struct S23HeldItemChange {
    pub slot: i16,
}

impl ServerBoundPacket for S23HeldItemChange {
    fn decode(mut decoder: PacketDecoder) -> DecodeResult<Self> {
        Ok(S23HeldItemChange {
            slot: decoder.read_short()?,
        })
    }
}

pub struct S26CreativeInventoryAction {
    pub slot: i16,
    pub clicked_item: Option<SlotData>,
}

impl ServerBoundPacket for S26CreativeInventoryAction {
    fn decode(mut decoder: PacketDecoder) -> DecodeResult<Self> {
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
        Ok(S26CreativeInventoryAction { slot, clicked_item })
    }
}
