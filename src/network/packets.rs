use byteorder::{BigEndian, WriteBytesExt};
use flate2::bufread::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Read;

pub struct PacketDecoder {
    buffer: Vec<u8>,
    i: usize,
    pub packet_id: u32,
}

impl PacketDecoder {
    pub fn decode(compression: bool, buf: Vec<u8>) -> Vec<PacketDecoder> {
        let mut decoders = Vec::new();
        let mut i = 0;
        while i < buf.len() {
            let length = PacketDecoder::read_varint_from_buffer(i, &buf);
            i += length.1 as usize;
            if compression {
                let data_length = PacketDecoder::read_varint_from_buffer(i, &buf);
                i += data_length.1 as usize;
                let mut data = Vec::new();
                // Decompress data
                ZlibDecoder::new(&buf[i..i + (length.0 - data_length.1) as usize])
                    .read_to_end(&mut data)
                    .unwrap();
                i += (length.0 - data_length.1) as usize;
                let packet_id = PacketDecoder::read_varint_from_buffer(0, &data);
                decoders.push(PacketDecoder {
                    buffer: Vec::from(&data[packet_id.1 as usize..data_length.0 as usize]),
                    i: 0,
                    packet_id: packet_id.0 as u32,
                });
            } else {
                let packet_id = PacketDecoder::read_varint_from_buffer(i, &buf);
                decoders.push(PacketDecoder {
                    buffer: Vec::from(&buf[i..i + length.0 as usize]),
                    i: 0,
                    packet_id: packet_id.0 as u32,
                });
                i += length.0 as usize;
            }
        }
        decoders
    }

    fn read_unsigned_byte(&mut self) -> u8 {
        self.i += 1;
        self.buffer[self.i - 1]
    }

    fn read_byte(&mut self) -> i8 {
        self.i += 1;
        self.buffer[self.i - 1] as i8
    }

    fn read_bytes(&mut self, bytes: usize) -> Vec<u8> {
        let out = &self.buffer[self.i..self.i + bytes];
        self.i += bytes;
        out.to_vec()
    }

    fn read_long(&mut self) -> i64 {
        let mut arr = [0; 8];
        arr.copy_from_slice(&self.buffer[self.i..self.i + 8]);
        let out = i64::from_be_bytes(arr);
        self.i += 8;
        out
    }

    fn read_int(&mut self) -> i32 {
        let mut arr = [0; 4];
        arr.copy_from_slice(&self.buffer[self.i..self.i + 4]);
        let out = i32::from_be_bytes(arr);
        self.i += 4;
        out
    }

    fn read_bool(&mut self) -> bool {
        let out = self.buffer[self.i] == 1;
        self.i += 1;
        out
    }

    fn read_varint_from_buffer(offset: usize, buf: &Vec<u8>) -> (i32, i32) {
        let mut num_read = 0;
        let mut result = 0i32;
        let mut read;
        loop {
            read = buf[offset + num_read as usize] as u8;
            let value = (read & 0b01111111) as i32;
            result |= value << (7 * num_read);

            num_read += 1;
            if num_read > 5 {
                panic!("VarInt is too big!");
            }
            if read & 0b10000000 == 0 {
                break;
            }
        }
        (result, num_read)
    }

    fn read_varint(&mut self) -> i32 {
        let mut num_read = 0;
        let mut result = 0i32;
        let mut read;
        loop {
            read = self.read_byte() as u8;
            let value = (read & 0b01111111) as i32;
            result |= value << (7 * num_read);

            num_read += 1;
            if num_read > 5 {
                panic!("VarInt is too big!");
            }
            if read & 0b10000000 == 0 {
                break;
            }
        }
        result
    }

    fn read_varlong(&mut self) -> i64 {
        let mut num_read = 0;
        let mut result = 0i64;
        let mut read;
        loop {
            read = self.read_byte() as u8;
            let value = (read & 0b01111111) as i64;
            result |= value << (7 * num_read);

            num_read += 1;
            if num_read > 5 {
                panic!("VarInt is too big!");
            }
            if read & 0b10000000 == 0 {
                break;
            }
        }
        result
    }

    fn read_string(&mut self) -> String {
        let length = self.read_varint();
        String::from_utf8(self.read_bytes(length as usize)).unwrap()
    }

    fn read_unsigned_short(&mut self) -> u16 {
        let mut arr = [0; 2];
        arr.copy_from_slice(&self.buffer[self.i..self.i + 2]);
        let out = u16::from_be_bytes(arr);
        self.i += 2;
        out
    }
}

pub struct PacketEncoder {
    buffer: Vec<u8>,
    packet_id: u32,
}

impl PacketEncoder {
    fn new(packet_id: u32) -> PacketEncoder {
        PacketEncoder {
            buffer: Vec::new(),
            packet_id,
        }
    }

    fn write_boolean(&mut self, val: bool) {
        self.buffer.push(val as u8);
    }

    fn write_varint(&mut self, val: i32) {
        self.buffer.append(&mut self.varint(val));
    }

    fn write_varlong(&mut self, mut val: i64) {
        loop {
            let mut temp = (val & 0b11111111) as u8;
            val = val >> 7;
            if val != 0 {
                temp |= 0b10000000;
            }
            self.buffer.push(temp);
            if val == 0 {
                break;
            }
        }
    }

    fn write_byte(&mut self, val: i8) {
        self.buffer.push(val as u8)
    }

    fn write_unsigned_byte(&mut self, val: u8) {
        self.buffer.push(val);
    }

    fn write_short(&mut self, val: i16) {
        self.buffer.write_i16::<BigEndian>(val).unwrap()
    }

    fn write_unsigned_short(&mut self, val: u16) {
        self.buffer.write_u16::<BigEndian>(val).unwrap()
    }

    fn write_int(&mut self, val: i32) {
        self.buffer.write_i32::<BigEndian>(val).unwrap()
    }
    fn write_double(&mut self, val: f32) {
        self.buffer.write_f32::<BigEndian>(val).unwrap()
    }

    fn write_string(&mut self, n: usize, val: String) {
        if val.len() > n * 4 + 3 {
            panic!("Tried to write string longer than the max length!");
        }
        self.buffer.append(&mut Vec::from(val.as_bytes()))
    }

    // This function is seperate because it is needed when writing packet headers
    fn varint(&self, mut val: i32) -> Vec<u8> {
        let mut buf = Vec::new();
        loop {
            let mut temp = (val & 0b11111111) as u8;
            val = val >> 7;
            if val != 0 {
                temp |= 0b10000000;
            }
            buf.push(temp);
            if val == 0 {
                return buf;
            }
        }
    }

    fn write_long(&mut self, val: i64) {
        self.buffer.write_i64::<BigEndian>(val).unwrap()
    }

    pub fn compressed(&self) -> Vec<u8> {
        let packet_id = self.varint(self.packet_id as i32);
        let data = [&packet_id[..], &self.buffer[..]].concat();
        let data_length = self.varint(data.len() as i32);
        let compressed = ZlibEncoder::new(data, Compression::default())
            .finish()
            .unwrap();
        let packet_length = self.varint((data_length.len() + compressed.len()) as i32);

        [&packet_length[..], &data_length[..], &compressed[..]].concat()
    }

    pub fn uncompressed(&self) -> Vec<u8> {
        let packet_id = self.varint(self.packet_id as i32);
        let length = self.varint((self.buffer.len() + packet_id.len()) as i32);

        [&length[..], &packet_id[..], &self.buffer[..]].concat()
    }
}

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
    pub username: String
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

pub struct C03SetCompression {
    pub threshold: i32
}

impl ClientBoundPacket for C03SetCompression {
    fn encode(self) -> PacketEncoder {
        let mut encoder = PacketEncoder::new(0x03);
        encoder.write_varint(self.threshold);
        encoder
    }
}

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