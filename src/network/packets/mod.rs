use byteorder::{BigEndian, WriteBytesExt};
use flate2::bufread::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::{self, Read, Write};

pub mod clientbound;
pub mod serverbound;

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
                // Compression is enabled
                let data_length = PacketDecoder::read_varint_from_buffer(i, &buf);
                i += data_length.1 as usize;
                if data_length.0 > 0 {
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
                    // Even though compression is enabled, packet is not compressed
                    let packet_id = PacketDecoder::read_varint_from_buffer(i, &buf);
                    i += packet_id.1 as usize;
                    let data = &buf
                        [i..i + length.0 as usize - data_length.1 as usize - packet_id.1 as usize];
                    decoders.push(PacketDecoder {
                        buffer: Vec::from(data),
                        i: 0,
                        packet_id: packet_id.0 as u32,
                    });
                    i += length.0 as usize - data_length.1 as usize - packet_id.1 as usize;
                }
            } else {
                // Compression is disabled
                let packet_id = PacketDecoder::read_varint_from_buffer(i, &buf);
                decoders.push(PacketDecoder {
                    buffer: Vec::from(&buf[i + 1..i + length.0 as usize]),
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

    fn read_short(&mut self) -> i16 {
        let mut arr = [0; 2];
        arr.copy_from_slice(&self.buffer[self.i..self.i + 2]);
        let out = i16::from_be_bytes(arr);
        self.i += 2;
        out
    }

    fn read_double(&mut self) -> f64 {
        let mut arr = [0; 8];
        arr.copy_from_slice(&self.buffer[self.i..self.i + 8]);
        let out = f64::from_be_bytes(arr);
        self.i += 8;
        out
    }

    fn read_float(&mut self) -> f32 {
        let mut arr = [0; 4];
        arr.copy_from_slice(&self.buffer[self.i..self.i + 4]);
        let out = f32::from_be_bytes(arr);
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

pub trait PacketEncoderExt: Write {
    fn write_boolean(&mut self, val: bool) {
        self.write_all(&[val as u8]).unwrap();
    }
    fn write_bytes(&mut self, val: &Vec<u8>) {
        self.write_all(&val).unwrap();
    }
    fn write_varint(&mut self, val: i32) {
        self.write_all(&PacketEncoder::varint(val));
    }

    fn write_varlong(&mut self, mut val: i64) {
        loop {
            let mut temp = (val & 0b11111111) as u8;
            val = val >> 7;
            if val != 0 {
                temp |= 0b10000000;
            }
            self.write_all(&[temp]).unwrap();
            if val == 0 {
                break;
            }
        }
    }

    fn write_byte(&mut self, val: i8) {
        self.write_all(&[val as u8]).unwrap();
    }

    fn write_unsigned_byte(&mut self, val: u8) {
        self.write_all(&[val]).unwrap();
    }

    fn write_short(&mut self, val: i16) {
        self.write_i16::<BigEndian>(val).unwrap()
    }

    fn write_unsigned_short(&mut self, val: u16) {
        self.write_u16::<BigEndian>(val).unwrap()
    }

    fn write_int(&mut self, val: i32) {
        self.write_i32::<BigEndian>(val).unwrap()
    }

    fn write_double(&mut self, val: f64) {
        self.write_f64::<BigEndian>(val).unwrap()
    }

    fn write_float(&mut self, val: f32) {
        self.write_f32::<BigEndian>(val).unwrap()
    }

    fn write_string(&mut self, n: usize, val: String) {
        if val.len() > n * 4 + 3 {
            panic!("Tried to write string longer than the max length!");
        }
        self.write_varint(val.len() as i32);
        self.write_all(val.as_bytes()).unwrap();
    }

    fn write_uuid(&mut self, val: u128) {
        self.write_u128::<BigEndian>(val).unwrap();
    }

    fn write_long(&mut self, val: i64) {
        self.write_i64::<BigEndian>(val).unwrap()
    }
}

impl PacketEncoderExt for Vec<u8> {}

pub struct PacketEncoder {
    buffer: Vec<u8>,
    packet_id: u32,
}

impl PacketEncoder {
    fn new(buffer: Vec<u8>, packet_id: u32) -> PacketEncoder {
        PacketEncoder { buffer, packet_id }
    }

    // This function is seperate because it is needed when writing packet headers
    fn varint(mut val: i32) -> Vec<u8> {
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

    pub fn compressed(&self) -> Vec<u8> {
        let packet_id = PacketEncoder::varint(self.packet_id as i32);
        let data = [&packet_id[..], &self.buffer[..]].concat();
        if self.buffer.len() < 500 {
            let data_length = PacketEncoder::varint(0);
            let packet_length = PacketEncoder::varint((data_length.len() + data.len()) as i32);
            [&packet_length[..], &data_length[..], &data[..]].concat()
        } else {
            let data_length = PacketEncoder::varint(data.len() as i32);
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&data).unwrap();
            let compressed = encoder.finish().unwrap();
            let packet_length =
                PacketEncoder::varint((data_length.len() + compressed.len()) as i32);

            [&packet_length[..], &data_length[..], &compressed[..]].concat()
        }
    }

    pub fn uncompressed(&self) -> Vec<u8> {
        let packet_id = PacketEncoder::varint(self.packet_id as i32);
        let length = PacketEncoder::varint((self.buffer.len() + packet_id.len()) as i32);

        [&length[..], &packet_id[..], &self.buffer[..]].concat()
    }
}
