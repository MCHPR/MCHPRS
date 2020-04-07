use super::PacketDecoder;

pub trait ServerBoundPacket {
    fn decode(decoder: PacketDecoder) -> Self;
}

pub struct S00Handshake {
    pub protocol_version: i32,
    pub server_address: String,
    pub server_port: u16,
    pub next_state: i32,
}

impl ServerBoundPacket for S00Handshake {
    fn decode(mut decoder: PacketDecoder) -> Self {
        S00Handshake {
            protocol_version: decoder.read_varint(),
            server_address: decoder.read_string(),
            server_port: decoder.read_unsigned_short(),
            next_state: decoder.read_varint(),
        }
    }
}

pub struct S00Request {}

impl ServerBoundPacket for S00Request {
    fn decode(mut _decoder: PacketDecoder) -> Self {
        S00Request {}
    }
}

pub struct S00Ping {
    pub payload: i64,
}

impl ServerBoundPacket for S00Ping {
    fn decode(mut decoder: PacketDecoder) -> Self {
        S00Ping {
            payload: decoder.read_long(),
        }
    }
}

pub struct S00LoginStart {
    pub name: String,
}

impl ServerBoundPacket for S00LoginStart {
    fn decode(mut decoder: PacketDecoder) -> Self {
        S00LoginStart {
            name: decoder.read_string(),
        }
    }
}

pub struct S03ChatMessage {
    pub message: String,
}

impl ServerBoundPacket for S03ChatMessage {
    fn decode(mut decoder: PacketDecoder) -> Self {
        S03ChatMessage {
            message: decoder.read_string(),
        }
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
    fn decode(mut decoder: PacketDecoder) -> Self {
        S05ClientSettings {
            locale: decoder.read_string(),
            view_distance: decoder.read_byte(),
            chat_mode: decoder.read_varint(),
            chat_colors: decoder.read_bool(),
            displayed_skin_parts: decoder.read_unsigned_byte(),
            main_hand: decoder.read_varint(),
        }
    }
}

pub struct S0FKeepAlive {
    pub id: i64,
}

impl ServerBoundPacket for S0FKeepAlive {
    fn decode(mut decoder: PacketDecoder) -> Self {
        S0FKeepAlive {
            id: decoder.read_long(),
        }
    }
}

pub struct S11PlayerPosition {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub on_ground: bool,
}

impl ServerBoundPacket for S11PlayerPosition {
    fn decode(mut decoder: PacketDecoder) -> Self {
        S11PlayerPosition {
            x: decoder.read_double(),
            y: decoder.read_double(),
            z: decoder.read_double(),
            on_ground: decoder.read_bool(),
        }
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
    fn decode(mut decoder: PacketDecoder) -> Self {
        S12PlayerPositionAndRotation {
            x: decoder.read_double(),
            y: decoder.read_double(),
            z: decoder.read_double(),
            yaw: decoder.read_float(),
            pitch: decoder.read_float(),
            on_ground: decoder.read_bool(),
        }
    }
}

pub struct S13PlayerRotation {
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
}

impl ServerBoundPacket for S13PlayerRotation {
    fn decode(mut decoder: PacketDecoder) -> Self {
        S13PlayerRotation {
            yaw: decoder.read_float(),
            pitch: decoder.read_float(),
            on_ground: decoder.read_bool(),
        }
    }
}

pub struct S14PlayerMovement {
    pub on_ground: bool,
}

impl ServerBoundPacket for S14PlayerMovement {
    fn decode(mut decoder: PacketDecoder) -> Self {
        S14PlayerMovement {
            on_ground: decoder.read_bool(),
        }
    }
}
