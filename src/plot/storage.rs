
use super::TickEntry;
use crate::blocks::{BlockEntity, BlockPos};
use crate::network::packets::clientbound::{C22ChunkData, C22ChunkDataSection, ClientBoundPacket};
use crate::network::packets::PacketEncoder;

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::mem;

#[derive(Debug, Serialize, Deserialize)]
pub struct PlotData {
    pub tps: u32,
    pub show_redstone: bool,
    pub chunk_data: Vec<ChunkData>,
    pub pending_ticks: Vec<TickEntry>,
}

#[derive(Debug, Clone)]
struct BitBuffer {
    bits_per_entry: u8,
    entries: usize,
    longs: Vec<u64>,
}

impl BitBuffer {
    fn create(bits_per_entry: u8, entries: usize) -> BitBuffer {
        let longs_len = (entries * bits_per_entry as usize + 63) / 64;
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

    fn get_entry(&self, word_idx: usize) -> u32 {

        // Find the set of indices.
        let abs_idx = word_idx * self.bits_per_entry as usize;
        let arr_idx = abs_idx >> 6;
        let sub_idx = abs_idx & 0x3f;

        // Find (at least) the lower half of the word, if not the full thing.
        let mask = (1 << self.bits_per_entry) - 1;
        let word = (self.longs[arr_idx] >> sub_idx) & mask;

        // If it's not on a boundary, we can early exit; there's no top half to fill in.
        if sub_idx + self.bits_per_entry as usize <= 64 {
            return word as u32;
        }

        // Otherwise, we need to get a little tricky.
        let bits_we_have = 64 - sub_idx;
        let next = self.longs[arr_idx + 1] << bits_we_have;
        ((word | next) & mask) as u32
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

#[derive(Debug, Clone)]
pub struct PalettedBitBuffer {
    data: BitBuffer,
    palette: Vec<u32>,
    max_entries: u32,
    use_palette: bool,
}

impl PalettedBitBuffer {
    pub fn new() -> PalettedBitBuffer {
        Self::with_entries(4096)
    }

    pub fn with_entries(entries: usize) -> PalettedBitBuffer {
        let mut palette = Vec::new();
        palette.push(0);
        PalettedBitBuffer {
            data: BitBuffer::create(4, entries),
            palette,
            max_entries: 16,
            use_palette: true,
        }
    }

    fn load(bits_per_entry: u8, longs: Vec<u64>, palette: Vec<u32>) -> PalettedBitBuffer {
        PalettedBitBuffer {
            data: BitBuffer::load(bits_per_entry, longs),
            palette,
            use_palette: bits_per_entry < 9,
            max_entries: 1 << bits_per_entry,
        }
    }

    fn resize_buffer(&mut self) {
        let old_bits_per_entry = self.data.bits_per_entry;
        if old_bits_per_entry + 1 > 8 {
            let mut old_buffer = BitBuffer::create(14, self.data.entries);
            mem::swap(&mut self.data, &mut old_buffer);
            self.max_entries = 1 << 14;
            for entry_idx in 0..old_buffer.entries {
                let entry = self.palette[old_buffer.get_entry(entry_idx) as usize];
                self.data.set_entry(entry_idx, entry);
            }
            self.use_palette = false;
        } else {
            let mut old_buffer = BitBuffer::create(old_bits_per_entry + 1, self.data.entries);
            mem::swap(&mut self.data, &mut old_buffer);
            self.max_entries <<= 1;
            for entry_idx in 0..old_buffer.entries {
                let entry = old_buffer.get_entry(entry_idx);
                self.data.set_entry(entry_idx, entry);
            }
        };
    }

    pub fn get_entry(&self, index: usize) -> u32 {
        if self.use_palette {
            self.palette[self.data.get_entry(index) as usize]
        } else {
            self.data.get_entry(index)
        }
    }

    pub fn set_entry(&mut self, index: usize, val: u32) {
        if self.use_palette {
            if let Some(palette_index) = self.palette.iter().position(|x| x == &val) {
                self.data.set_entry(index, palette_index as u32);
            } else {
                if self.palette.len() + 1 > self.max_entries as usize {
                    self.resize_buffer();
                    self.set_entry(index, val);
                    return;
                }
                let palette_index = self.palette.len();
                self.palette.push(val);
                self.data.set_entry(index, palette_index as u32);
            }
        } else {
            self.data.set_entry(index, val);
        }
    }

    pub fn entries(&self) -> usize {
        self.data.entries
    }
}

#[derive(Debug)]
pub struct ChunkSection {
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
        let palette = data.palette.into_iter().map(|x| x as u32).collect();
        let buffer = PalettedBitBuffer::load(bits_per_entry, loaded_longs, palette);

        ChunkSection {
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

        let palette: Vec<i32> = self
            .buffer
            .palette
            .clone()
            .into_iter()
            .map(|x| x as i32)
            .collect();

        ChunkSectionData {
            data: longs,
            palette,
            bits_per_block: self.buffer.data.bits_per_entry as i8,
            block_count: self.block_count as i32,
        }
    }

    fn new() -> ChunkSection {
        ChunkSection {
            buffer: PalettedBitBuffer::new(),
            block_count: 10,
        }
    }

    fn encode_packet(&self) -> C22ChunkDataSection {
        C22ChunkDataSection {
            bits_per_block: self.buffer.data.bits_per_entry,
            block_count: self.block_count as i16,
            data_array: self.buffer.data.longs.clone(),
            palette: if self.buffer.use_palette {
                Some(
                    self.buffer
                        .palette
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
    pub sections: BTreeMap<u8, ChunkSection>,
    pub x: i32,
    pub z: i32,
    pub block_entities: HashMap<BlockPos, BlockEntity>,
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

        for (section_y, section) in &self.sections {
            bitmask |= 1 << section_y;
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

        let mut block_entities = Vec::new();
        self.block_entities
            .iter()
            .map(|(pos, block_entity)| {
                block_entity
                    .to_nbt(BlockPos::new(
                        pos.x + (self.x << 4),
                        pos.y,
                        pos.z + (self.z << 4),
                    ))
                    .map(|blob| block_entities.push(blob))
            })
            .for_each(drop);
            
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
            block_entities,
        }
        .encode()
    }

    fn get_top_most_block(&self, x: u32, z: u32) -> u32 {
        let mut top_most = 0;
        for (section_y, section) in &self.sections {
            for y in (0..15).rev() {
                let block_state = section.get_block(x, y, z);
                if block_state != 0 && top_most < y + *section_y as u32 * 16 {
                    top_most = *section_y as u32 * 16;
                }
            }
        }
        top_most
    }

    /// Sets a block in the chunk. Returns true if a block was changed.
    pub fn set_block(&mut self, x: u32, y: u32, z: u32, block_id: u32) -> bool {
        let section_y = (y >> 4) as u8;
        if let Some(section) = self.sections.get_mut(&section_y) {
            section.set_block(x, y & 0xF, z, block_id)
        } else if block_id != 0 {
            let mut section = ChunkSection::new();
            section.set_block(x, y & 0xF, z, block_id);
            self.sections.insert(section_y, section);
            true
        } else {
            // The block was air so a new chunk section does not need to be created.
            false
        }
    }

    pub fn get_block(&self, x: u32, y: u32, z: u32) -> u32 {
        let section_y = (y / 16) as u8;
        if let Some(section) = self.sections.get(&section_y) {
            section.get_block(x, y & 0xF, z)
        } else {
            0
        }
    }

    pub fn get_block_entity(&self, pos: BlockPos) -> Option<&BlockEntity> {
        self.block_entities.get(&pos)
    }

    pub fn delete_block_entity(&mut self, pos: BlockPos) {
        self.block_entities.remove(&pos);
    }

    pub fn set_block_entity(&mut self, pos: BlockPos, block_entity: BlockEntity) {
        self.block_entities.insert(pos, block_entity);
    }

    pub fn save(&self) -> ChunkData {
        ChunkData {
            sections: self.sections.iter().map(|(y, s)| (*y, s.save())).collect(),
            block_entities: self.block_entities.clone(),
        }
    }

    pub fn load(x: i32, z: i32, chunk_data: ChunkData) -> Chunk {
        Chunk {
            x,
            z,
            sections: chunk_data
                .sections
                .into_iter()
                .map(|(y, cs)| (y, ChunkSection::load(cs)))
                .collect(),
            block_entities: chunk_data.block_entities,
        }
    }

    pub fn empty(x: i32, z: i32) -> Chunk {
        Chunk {
            sections: BTreeMap::new(),
            x,
            z,
            block_entities: HashMap::new(),
        }
    }

    pub fn generate(layers: i32, x: i32, z: i32) -> Chunk {
        let mut chunk = Chunk {
            sections: BTreeMap::new(),
            x,
            z,
            block_entities: HashMap::new(),
        };

        for ry in 0..layers {
            for rx in 0..16 {
                for rz in 0..16 {
                    let block_x = (x << 4) | rx;
                    let block_z = (z << 4) | rz;

                    if block_x % 256 == 0
                        || block_z % 256 == 0
                        || (block_x + 1) % 256 == 0
                        || (block_z + 1) % 256 == 0
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
    data: Vec<i64>,
    palette: Vec<i32>,
    bits_per_block: i8,
    block_count: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChunkData {
    sections: BTreeMap<u8, ChunkSectionData>,
    block_entities: HashMap<BlockPos, BlockEntity>,
}
