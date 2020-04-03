use super::PacketEncoder;

pub trait ClientBoundPacket {
    fn encode(self) -> PacketEncoder;
}

pub struct C00Reponse {
    pub json_response: String,
}

impl ClientBoundPacket for C00Reponse {
    fn encode(self) -> PacketEncoder {
        let mut encoder = PacketEncoder::new(0x00);
        encoder.write_string(32767, self.json_response);
        encoder
    }
}

pub struct C01Pong {
    pub payload: i64,
}

impl ClientBoundPacket for C01Pong {
    fn encode(self) -> PacketEncoder {
        let mut encoder = PacketEncoder::new(0x01);
        encoder.write_long(self.payload);
        encoder
    }
}

pub struct C02LoginSuccess {
    pub uuid: u128,
    pub username: String,
}

impl ClientBoundPacket for C02LoginSuccess {
    fn encode(self) -> PacketEncoder {
        let mut encoder = PacketEncoder::new(0x02);
        let mut hex = format!("{:032X}", self.uuid);
        hex.insert(7, '-');
        hex.insert(13, '-');
        hex.insert(17, '-');
        hex.insert(21, '-');
        encoder.write_string(36, hex);
        encoder.write_string(16, self.username);
        encoder
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
        let mut encoder = PacketEncoder::new(0x26);
        encoder.write_int(self.entity_id);
        encoder.write_unsigned_byte(self.gamemode);
        encoder.write_int(self.dimention);
        encoder.write_long(self.hash_seed);
        encoder.write_unsigned_byte(self.max_players);
        encoder.write_string(16, self.level_type);
        encoder.write_varint(self.view_distance);
        encoder.write_boolean(self.reduced_debug_info);
        encoder.write_boolean(self.enable_respawn_screen);
        encoder
    }
}

pub struct C03SetCompression {
    pub threshold: i32,
}

impl ClientBoundPacket for C03SetCompression {
    fn encode(self) -> PacketEncoder {
        let mut encoder = PacketEncoder::new(0x03);
        encoder.write_varint(self.threshold);
        encoder
    }
}
