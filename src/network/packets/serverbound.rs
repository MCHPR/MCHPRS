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
