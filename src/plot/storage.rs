use crate::network::packets::clientbound::{C22ChunkData, C22ChunkDataSection, ClientBoundPacket};
use crate::network::packets::PacketEncoder;
use serde::{Deserialize, Serialize};
use std::mem;

#[derive(Debug, Serialize, Deserialize)]
pub struct PlotData {
    pub tps: i32,
    pub show_redstone: bool,
    pub chunk_data: Vec<ChunkData>,
}

#[derive(Debug)]
struct BitBuffer {
    bits_per_entry: u8,
    entries: usize,
    longs: Vec<u64>,
}

impl BitBuffer {
    fn create(bits_per_entry: u8, entries: usize) -> BitBuffer {
        let longs_len = entries * bits_per_entry as usize / 64;
        let longs = vec![0; longs_len];
        BitBuffer {
            bits_per_entry,
            longs,
            entries,
        }
    }

    fn load(bits_per_entry: u8, longs: Vec<u64>) -> BitBuffer {
        let entries = longs.len() * 64 / bits_per_entry as usize;
        BitBuffer {
            bits_per_entry,
            longs,
            entries,
        }
    }

    fn get_entry(&self, index: usize) -> u32 {
        let long_index = (self.bits_per_entry as usize * index) >> 6;
        let index_in_long = (self.bits_per_entry as usize * index) & 0x3F;
        let bitmask = ((1u128 << self.bits_per_entry) - 1) << index_in_long;

        let mut long_long = self.longs[long_index] as u128;
        if self.longs.len() > long_index + 1 {
            long_long |= (self.longs[long_index + 1] as u128) << 64;
        }
        // if ((bitmask & long_long) >> index_in_long) != 0 {
        //     println!("long:    {:0128b}\nbitmask: {:128b} {}", long_long, bitmask, self.bits_per_entry);
        // }
        ((bitmask & long_long) >> index_in_long) as u32
    }

    fn set_entry(&mut self, index: usize, val: u32) {
        let long_index = (self.bits_per_entry as usize * index) >> 6;
        let index_in_long = (self.bits_per_entry as usize * index) & 0x3F;
        let bitmask = ((1u128 << self.bits_per_entry) - 1) << index_in_long;

        self.longs[long_index] = (self.longs[long_index] & !(bitmask as u64)) // Remove old value
            | ((val as u128) << index_in_long as u128) as u64; // Insert new value, TODO: use a better way than `as u128`

        // Check if the value overlaps into the next long
        if index_in_long + self.bits_per_entry as usize > 64 {
            self.longs[long_index + 1] = (self.longs[long_index + 1] & !(bitmask >> 64) as u64) // Remove old value
                | (val >> (64 - index_in_long)) as u64; // Insert new value
        }
    }
}

#[derive(Debug)]
struct PalettedBitBuffer {
    data: BitBuffer,
    palatte: Vec<u32>,
    max_entries: u32,
    use_palatte: bool,
}

impl PalettedBitBuffer {
    fn new() -> PalettedBitBuffer {
        let mut palatte = Vec::new();
        palatte.push(0);
        PalettedBitBuffer {
            data: BitBuffer::create(4, 4096),
            palatte,
            max_entries: 16,
            use_palatte: true,
        }
    }

    fn load(bits_per_entry: u8, longs: Vec<u64>, palatte: Vec<u32>) -> PalettedBitBuffer {
        PalettedBitBuffer {
            data: BitBuffer::load(bits_per_entry, longs),
            palatte,
            use_palatte: bits_per_entry < 9,
            max_entries: 1 << bits_per_entry,
        }
    }

    fn resize_buffer(&mut self) {
        let old_bits_per_entry = self.data.bits_per_entry;
        if old_bits_per_entry + 1 > 8 {
            let mut old_buffer = BitBuffer::create(14, 4096);
            mem::swap(&mut self.data, &mut old_buffer);
            self.max_entries = 1 << 14;
            for entry in 0..old_buffer.entries {
                self.data
                    .set_entry(entry, self.palatte[old_buffer.get_entry(entry) as usize]);
            }
        } else {
            let mut old_buffer = BitBuffer::create(old_bits_per_entry + 1, 4096);
            mem::swap(&mut self.data, &mut old_buffer);
            self.max_entries <<= 1;
            for entry in 0..old_buffer.entries {
                self.data.set_entry(entry, old_buffer.get_entry(entry));
            }
        };
    }

    fn get_entry(&self, index: usize) -> u32 {
        if self.use_palatte {
            self.palatte[self.data.get_entry(index) as usize]
        } else {
            self.data.get_entry(index)
        }
    }

    fn set_entry(&mut self, index: usize, val: u32) {
        if self.use_palatte {
            if let Some(palatte_index) = self.palatte.iter().position(|x| x == &val) {
                self.data.set_entry(index, palatte_index as u32);
            } else {
                if self.palatte.len() + 1 > self.max_entries as usize {
                    self.resize_buffer();
                }
                let palatte_index = self.palatte.len();
                self.palatte.push(val);
                self.data.set_entry(index, palatte_index as u32);
            }
        } else {
            self.data.set_entry(index, val);
        }
    }
}

#[derive(Debug)]
struct ChunkSection {
    y: u8,
    buffer: PalettedBitBuffer,
    block_count: u32,
}

impl ChunkSection {
    fn get_index(x: u32, y: u32, z: u32) -> usize {
        ((y << 8) | (z << 4) | x) as usize
    }

    fn get_block(&self, x: u32, y: u32, z: u32) -> u32 {
        self.buffer.get_entry(ChunkSection::get_index(x, y, z))
    }

    /// Sets a block in the chunk sections. Returns true if a block was changed.
    fn set_block(&mut self, x: u32, y: u32, z: u32, block: u32) -> bool {
        let old_block = self.get_block(x, y, z);
        if old_block == 0 && block != 0 {
            self.block_count += 1;
        } else if old_block != 0 && block == 0 {
            self.block_count -= 1;
        }
        self.buffer
            .set_entry(ChunkSection::get_index(x, y, z), block);
        old_block != block
    }

    fn load(data: ChunkSectionData) -> ChunkSection {
        let loaded_longs = data.data.into_iter().map(|x| x as u64).collect();
        let bits_per_entry = data.bits_per_block as u8;
        let palette = data.palatte.into_iter().map(|x| x as u32).collect();
        let buffer = PalettedBitBuffer::load(bits_per_entry, loaded_longs, palette);
        ChunkSection {
            y: data.y as u8,
            buffer,
            block_count: data.block_count as u32,
        }
    }

    fn save(&self) -> ChunkSectionData {
        let longs: Vec<i64> = self
            .buffer
            .data
            .longs
            .clone()
            .into_iter()
            .map(|x| x as i64)
            .collect();
        let palatte: Vec<i32> = self
            .buffer
            .palatte
            .clone()
            .into_iter()
            .map(|x| x as i32)
            .collect();
        ChunkSectionData {
            data: longs,
            palatte,
            bits_per_block: self.buffer.data.bits_per_entry as i8,
            y: self.y as i8,
            block_count: self.block_count as i32,
        }
    }

    fn new(y: u8) -> ChunkSection {
        ChunkSection {
            y,
            buffer: PalettedBitBuffer::new(),
            block_count: 10,
        }
    }

    fn encode_packet(&self) -> C22ChunkDataSection {
        C22ChunkDataSection {
            bits_per_block: self.buffer.data.bits_per_entry,
            block_count: self.block_count as i16,
            data_array: self.buffer.data.longs.clone(),
            palette: if self.buffer.use_palatte {
                Some(
                    self.buffer
                        .palatte
                        .clone()
                        .into_iter()
                        .map(|x| x as i32)
                        .collect(),
                )
            } else {
                None
            },
        }
    }
}

#[derive(Debug)]
pub struct Chunk {
    sections: Vec<ChunkSection>,
    x: i32,
    z: i32,
}

impl Chunk {
    pub fn encode_packet(&self, full_chunk: bool) -> PacketEncoder {
        let mut heightmap_buffer = BitBuffer::create(9, 256);
        for x in 0..16 {
            for z in 0..16 {
                heightmap_buffer
                    .set_entry((x * 16) + z, self.get_top_most_block(x as u32, z as u32));
            }
        }

        let mut chunk_sections = Vec::new();
        let mut bitmask = 0;
        for section in &self.sections {
            bitmask |= 1 << section.y;
            chunk_sections.push(section.encode_packet());
        }
        let mut heightmaps = nbt::Blob::new();
        let heightmap_longs: Vec<i64> = heightmap_buffer
            .longs
            .into_iter()
            .map(|x| x as i64)
            .collect();
        heightmaps
            .insert("MOTION_BLOCKING", heightmap_longs)
            .unwrap();
        C22ChunkData {
            // Use `bool_to_option` feature when stabalized
            // Tracking issue: https://github.com/rust-lang/rust/issues/64260
            biomes: if full_chunk {
                Some(vec![0; 1024])
            } else {
                None
            },
            chunk_sections,
            chunk_x: self.x,
            chunk_z: self.z,
            full_chunk,
            heightmaps,
            primary_bit_mask: bitmask as i32,
        }
        .encode()
    }

    fn get_top_most_block(&self, x: u32, z: u32) -> u32 {
        let mut top_most = 0;
        for section in &self.sections {
            for y in (0..15).rev() {
                let block_state = section.get_block(x, y, z);
                if block_state != 0 && top_most < y + section.y as u32 * 16 {
                    top_most = section.y as u32 * 16;
                }
            }
        }
        top_most
    }

    /// Sets a block in the chunk. Returns true if a block was changed.
    pub fn set_block(&mut self, x: u32, y: u32, z: u32, block_id: u32) -> bool {
        let section_y = (y >> 4) as u8;
        if let Some(section) = self.sections.iter_mut().find(|s| s.y == section_y) {
            section.set_block(x, y & 0xF, z, block_id)
        } else if block_id != 0 {
            let mut section = ChunkSection::new(section_y);
            section.set_block(x, y & 0xF, z, block_id);
            self.sections.push(section);
            true
        } else {
            // The block was air so a new chunk section does not need to be created.
            false
        }
    }

    pub fn get_block(&self, x: u32, y: u32, z: u32) -> u32 {
        let section_y = (y / 16) as u8;
        if let Some(section) = self.sections.iter().find(|s| s.y == section_y) {
            section.get_block(x, y & 0xF, z)
        } else {
            0
        }
    }

    pub fn save(&self) -> ChunkData {
        ChunkData {
            sections: self.sections.iter().map(|s| s.save()).collect(),
        }
    }

    pub fn load(x: i32, z: i32, chunk_data: ChunkData) -> Chunk {
        Chunk {
            x,
            z,
            sections: chunk_data
                .sections
                .into_iter()
                .map(ChunkSection::load)
                .collect(),
        }
    }

    pub fn empty(x: i32, z: i32) -> Chunk {
        Chunk {
            sections: Vec::new(),
            x,
            z,
        }
    }

    pub fn generate(layers: i32, x: i32, z: i32) -> Chunk {
        let mut chunk = Chunk {
            sections: Vec::new(),
            x,
            z,
        };

        for ry in 0..layers {
            for rx in 0..16 {
                for rz in 0..16 {
                    let block_x = (x << 4) | rx;
                    let block_z = (z << 4) | rz;

                    if block_x % 128 == 0
                        || block_z % 128 == 0
                        || (block_x + 1) % 128 == 0
                        || (block_z + 1) % 128 == 0
                    {
                        chunk.set_block(rx as u32, ry as u32, rz as u32, 4481); // Stone Bricks
                    } else {
                        chunk.set_block(rx as u32, ry as u32, rz as u32, 245); // Sandstone
                    }
                }
            }
        }
        chunk
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct ChunkSectionData {
    y: i8,
    data: Vec<i64>,
    palatte: Vec<i32>,
    bits_per_block: i8,
    block_count: i32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChunkData {
    sections: Vec<ChunkSectionData>,
}
