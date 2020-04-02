use flate2::write::ZlibEncoder;
use flate2::read::ZlibDecoder;
use flate2::Compression;
use byteorder::{BigEndian, WriteBytesExt};

struct PacketDecoder {
    buffer: Vec<u8>,
    i: usize,
    packet_id: u32
}

impl PacketDecoder {

    fn new() -> PacketDecoder {
        PacketDecoder {
            buffer: Vec::new(),
            i: 0,
            packet_id: 0
        }
    }

}

struct PacketEncoder {
    buffer: Vec<u8>,
    packet_id: u32
}

impl PacketEncoder {

    fn new(packet_id: u32) -> PacketEncoder {
        PacketEncoder {
            buffer: Vec::new(),
            packet_id
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

    fn compressed(&self) -> Vec<u8> {
        let packet_id = self.varint(self.packet_id as i32);
        let data = [&packet_id[..], &self.buffer[..]].concat();
        let data_length = self.varint(data.len() as i32);
        let compressed = ZlibEncoder::new(data, Compression::default()).finish().unwrap();
        let packet_length = self.varint((data_length.len() + compressed.len()) as i32);

        [&packet_length[..], &data_length[..], &compressed[..]].concat()
    }

    fn uncompressed(&self) -> Vec<u8> {
        let packet_id = self.varint(self.packet_id as i32);
        let length = self.varint((self.buffer.len() + packet_id.len()) as i32);

        [&length[..], &packet_id[..], &self.buffer[..]].concat()
    }
}

trait Packet {
    fn encode(self) -> PacketEncoder;
}