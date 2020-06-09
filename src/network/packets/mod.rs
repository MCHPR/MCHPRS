use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use flate2::bufread::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};

pub mod clientbound;
pub mod serverbound;

#[derive(Debug)]
pub struct SlotData {
    pub item_id: i32,
    pub item_count: i8,
    pub nbt: Option<nbt::Blob>,
}

pub type DecodeResult<T> = std::result::Result<T, PacketDecodeError>;
pub type EncodeResult<T> = std::result::Result<T, PacketEncodeError>;

#[derive(Debug)]
pub enum PacketDecodeError {
    IoError(io::Error),
    FromUtf8Error(std::string::FromUtf8Error),
    NbtError(nbt::Error),
}

impl From<nbt::Error> for PacketDecodeError {
    fn from(err: nbt::Error) -> PacketDecodeError {
        PacketDecodeError::NbtError(err)
    }
}

impl From<io::Error> for PacketDecodeError {
    fn from(err: io::Error) -> PacketDecodeError {
        PacketDecodeError::IoError(err)
    }
}

impl From<std::string::FromUtf8Error> for PacketDecodeError {
    fn from(err: std::string::FromUtf8Error) -> PacketDecodeError {
        PacketDecodeError::FromUtf8Error(err)
    }
}

#[derive(Debug)]
pub enum PacketEncodeError {}

pub struct PacketDecoder {
    buffer: Cursor<Vec<u8>>,
    pub packet_id: u32,
}

impl PacketDecoder {
    pub fn decode(compression: bool, buf: Vec<u8>) -> DecodeResult<Vec<PacketDecoder>> {
        let mut decoders = Vec::new();
        let mut i = 0;
        while i < buf.len() {
            let length = PacketDecoder::read_varint_from_buffer(i, &buf)?;
            i += length.1 as usize;
            if compression {
                // Compression is enabled
                let data_length = PacketDecoder::read_varint_from_buffer(i, &buf)?;
                i += data_length.1 as usize;
                if data_length.0 > 0 {
                    let mut data = Vec::new();
                    // Decompress data
                    ZlibDecoder::new(&buf[i..i + (length.0 - data_length.1) as usize])
                        .read_to_end(&mut data)
                        .unwrap();
                    i += (length.0 - data_length.1) as usize;
                    let packet_id = PacketDecoder::read_varint_from_buffer(0, &data)?;
                    decoders.push(PacketDecoder {
                        buffer: Cursor::new(Vec::from(
                            &data[packet_id.1 as usize..data_length.0 as usize],
                        )),
                        packet_id: packet_id.0 as u32,
                    });
                } else {
                    // Even though compression is enabled, packet is not compressed
                    let packet_id = PacketDecoder::read_varint_from_buffer(i, &buf)?;
                    i += packet_id.1 as usize;
                    let data = &buf
                        [i..i + length.0 as usize - data_length.1 as usize - packet_id.1 as usize];
                    decoders.push(PacketDecoder {
                        buffer: Cursor::new(Vec::from(data)),
                        packet_id: packet_id.0 as u32,
                    });
                    i += length.0 as usize - data_length.1 as usize - packet_id.1 as usize;
                }
            } else {
                // Compression is disabled
                let packet_id = PacketDecoder::read_varint_from_buffer(i, &buf)?;
                decoders.push(PacketDecoder {
                    buffer: Cursor::new(Vec::from(&buf[i + 1..i + length.0 as usize])),
                    packet_id: packet_id.0 as u32,
                });
                i += length.0 as usize;
            }
        }
        Ok(decoders)
    }

    fn read_unsigned_byte(&mut self) -> DecodeResult<u8> {
        Ok(self.buffer.read_u8()?)
    }

    fn read_byte(&mut self) -> DecodeResult<i8> {
        Ok(self.buffer.read_i8()?)
    }

    fn read_bytes(&mut self, bytes: usize) -> DecodeResult<Vec<u8>> {
        let mut read = vec![0; bytes];
        self.buffer.read_exact(&mut read)?;
        Ok(read)
    }

    fn read_long(&mut self) -> DecodeResult<i64> {
        Ok(self.buffer.read_i64::<BigEndian>()?)
    }

    fn read_int(&mut self) -> DecodeResult<i32> {
        Ok(self.buffer.read_i32::<BigEndian>()?)
    }

    fn read_short(&mut self) -> DecodeResult<i16> {
        Ok(self.buffer.read_i16::<BigEndian>()?)
    }

    fn read_unsigned_short(&mut self) -> DecodeResult<u16> {
        Ok(self.buffer.read_u16::<BigEndian>()?)
    }

    fn read_double(&mut self) -> DecodeResult<f64> {
        Ok(self.buffer.read_f64::<BigEndian>()?)
    }

    fn read_float(&mut self) -> DecodeResult<f32> {
        Ok(self.buffer.read_f32::<BigEndian>()?)
    }

    fn read_bool(&mut self) -> DecodeResult<bool> {
        Ok(self.buffer.read_u8()? == 1)
    }

    fn read_varint_from_buffer(offset: usize, buf: &[u8]) -> DecodeResult<(i32, i32)> {
        let mut num_read = 0;
        let mut result = 0i32;
        let mut read;
        loop {
            read = buf[offset + num_read as usize] as u8;
            let value = (read & 0b0111_1111) as i32;
            result |= value << (7 * num_read);

            num_read += 1;
            if num_read > 5 {
                panic!("VarInt is too big!");
            }
            if read & 0b1000_0000 == 0 {
                break;
            }
        }
        Ok((result, num_read))
    }

    fn read_varint(&mut self) -> DecodeResult<i32> {
        let mut num_read = 0;
        let mut result = 0i32;
        let mut read;
        loop {
            read = self.read_byte()? as u8;
            let value = (read & 0b0111_1111) as i32;
            result |= value << (7 * num_read);

            num_read += 1;
            if num_read > 5 {
                panic!("VarInt is too big!");
            }
            if read & 0b1000_0000 == 0 {
                break;
            }
        }
        Ok(result)
    }

    fn read_varlong(&mut self) -> DecodeResult<i64> {
        let mut num_read = 0;
        let mut result = 0i64;
        let mut read;
        loop {
            read = self.read_byte()? as u8;
            let value = (read & 0b0111_1111) as i64;
            result |= value << (7 * num_read);

            num_read += 1;
            if num_read > 5 {
                panic!("VarInt is too big!");
            }
            if read & 0b1000_0000 == 0 {
                break;
            }
        }
        Ok(result)
    }

    fn read_string(&mut self) -> DecodeResult<String> {
        let length = self.read_varint()?;
        Ok(String::from_utf8(self.read_bytes(length as usize)?)?)
    }

    fn read_to_end(&mut self) -> DecodeResult<Vec<u8>> {
        let mut data = Vec::new();
        self.buffer.read_to_end(&mut data)?;
        Ok(data)
    }

    fn read_position(&mut self) -> DecodeResult<(i32, i32, i32)> {
        let val: i64 = self.read_long()?;
        let x = val >> 38;
        let mut y = val & 0xFFF;
        if y >= 0x800 {
            y -= 0x1000
        }
        let z = val << 26 >> 38;
        Ok((x as i32, y as i32, z as i32))
    }

    fn read_nbt_blob(&mut self) -> DecodeResult<Option<nbt::Blob>> {
        if self.buffer.read_u8()? == 0x00 {
            return Ok(None);
        }
        self.buffer.seek(SeekFrom::Current(-1))?;
        Ok(Some(nbt::Blob::from_reader(&mut self.buffer)?))
    }
}

pub trait PacketEncoderExt: Write {
    fn write_boolean(&mut self, val: bool) {
        self.write_all(&[val as u8]).unwrap();
    }
    fn write_bytes(&mut self, val: Vec<u8>) {
        self.write_all(&val).unwrap();
    }
    fn write_varint(&mut self, val: i32) {
        self.write_all(&PacketEncoder::varint(val));
    }

    fn write_varlong(&mut self, mut val: i64) {
        loop {
            let mut temp = (val & 0b1111_1111) as u8;
            val >>= 7;
            if val != 0 {
                temp |= 0b1000_0000;
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

    fn write_string(&mut self, n: usize, val: &str) {
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

    fn write_position(&mut self, x: i32, y: i32, z: i32) {
        let long =
            ((x as i64 & 0x3FF_FFFF) << 38) | ((z as i64 & 0x3FF_FFFF) << 12) | (y as i64 & 0xFFF);
        self.write_long(long);
    }

    fn write_bool(&mut self, val: bool) {
        self.write_u8(val as u8).unwrap();
    }

    fn write_nbt_blob(&mut self, blob: nbt::Blob);
}

impl PacketEncoderExt for Vec<u8> {
    fn write_nbt_blob(&mut self, blob: nbt::Blob) {
        blob.to_writer(self).unwrap();
    }
}

pub struct PacketEncoder {
    buffer: Vec<u8>,
    packet_id: u32,
}

impl PacketEncoder {
    fn new(buffer: Vec<u8>, packet_id: u32) -> PacketEncoder {
        PacketEncoder { buffer, packet_id }
    }

    // This function is seperate because it is needed when writing packet headers
    fn varint(val: i32) -> Vec<u8> {
        let mut val = val as u32;
        let mut buf = Vec::new();
        loop {
            let mut temp = (val & 0b1111_1111) as u8;
            val = val >> 7;
            if val != 0 {
                temp |= 0b1000_0000;
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
