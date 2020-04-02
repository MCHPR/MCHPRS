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

    fn decode() -> Vec<PacketDecoder> {
        let decoders = Vec::new();
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
        return result;
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