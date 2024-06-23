use crate::plot::{PLOT_BLOCK_HEIGHT, PLOT_SECTIONS};
use itertools::Itertools;
use mchprs_blocks::block_entities::BlockEntity;
use mchprs_blocks::BlockPos;
use mchprs_save_data::plot_data::{ChunkData, ChunkSectionData};

use mchprs_network::packets::clientbound::{
    CChunkData, CChunkDataBlockEntity, CChunkDataSection, CUpdateSectionBlocks,
    CUpdateSectionBlocksRecord, ClientBoundPacket,
};
use mchprs_network::packets::{PacketEncoder, PalettedContainer};
use rustc_hash::FxHashMap;
use std::convert::TryInto;
use std::mem;

#[derive(Clone)]
pub struct BitBuffer {
    bits_per_entry: u64,
    entries_per_long: u64,
    entries: usize,
    mask: u64,
    longs: Vec<u64>,
    fast_arr_idx: fn(word_idx: usize) -> usize,
}

impl std::fmt::Debug for BitBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BitBuffer")
            .field("bits_per_entry", &self.bits_per_entry)
            .field("entries", &self.entries)
            .field("entries_per_long", &self.entries_per_long)
            .field("mask", &self.mask)
            .finish()
    }
}

impl BitBuffer {
    fn find_fast_arr_idx_fn(entries_per_long: usize) -> fn(word_idx: usize) -> usize {
        fn fast_arr_idx<const N: usize>(word_idx: usize) -> usize {
            word_idx / N
        }

        match entries_per_long {
            16 => fast_arr_idx::<16>,
            12 => fast_arr_idx::<12>,
            10 => fast_arr_idx::<10>,
            9 => fast_arr_idx::<9>,
            8 => fast_arr_idx::<8>,
            7 => fast_arr_idx::<7>,
            6 => fast_arr_idx::<6>,
            5 => fast_arr_idx::<5>,
            4 => fast_arr_idx::<4>,
            _ => unreachable!("entries_per_long cannot be {}", entries_per_long),
        }
    }

    pub fn create(bits_per_entry: u8, entries: usize) -> BitBuffer {
        // 4..9, 15
        let entries_per_long = 64 / bits_per_entry as u64;
        // Rounding up div
        let longs_len = (entries + entries_per_long as usize - 1) / entries_per_long as usize;
        let longs = vec![0; longs_len];
        BitBuffer {
            bits_per_entry: bits_per_entry as u64,
            longs,
            entries,
            entries_per_long,
            mask: (1 << bits_per_entry) - 1,
            fast_arr_idx: BitBuffer::find_fast_arr_idx_fn(entries_per_long as usize),
        }
    }

    fn load(entries: usize, bits_per_entry: u8, longs: Vec<u64>) -> BitBuffer {
        let entries_per_long = 64 / bits_per_entry as u64;
        BitBuffer {
            bits_per_entry: bits_per_entry as u64,
            longs,
            entries,
            entries_per_long,
            mask: (1 << bits_per_entry) - 1,
            fast_arr_idx: BitBuffer::find_fast_arr_idx_fn(entries_per_long as usize),
        }
    }

    pub fn get_entry(&self, word_idx: usize) -> u32 {
        // Find the set of indices.
        let arr_idx = (self.fast_arr_idx)(word_idx);
        let sub_idx =
            (word_idx as u64 - arr_idx as u64 * self.entries_per_long) * self.bits_per_entry;
        // Find the word.
        let word = (self.longs[arr_idx] >> sub_idx) & self.mask;
        word as u32
    }

    pub fn set_entry(&mut self, word_idx: usize, word: u32) {
        // Find the set of indices.
        let arr_idx = (self.fast_arr_idx)(word_idx);
        let sub_idx =
            (word_idx as u64 - arr_idx as u64 * self.entries_per_long) * self.bits_per_entry;
        // Set the word.
        let mask = !(self.mask << sub_idx);
        self.longs[arr_idx] = (self.longs[arr_idx] & mask) | ((word as u64) << sub_idx);
    }
}

#[test]
fn bitbuffer_format() {
    let entries = [
        1, 2, 2, 3, 4, 4, 5, 6, 6, 4, 8, 0, 7, 4, 3, 13, 15, 16, 9, 14, 10, 12, 0, 2,
    ];
    let mut buffer = BitBuffer::create(5, 24);
    for (i, entry) in entries.iter().enumerate() {
        buffer.set_entry(i, *entry);
    }
    assert_eq!(buffer.longs[0], 0x0020863148418841);
    assert_eq!(buffer.longs[1], 0x01018A7260F68C87);
}

#[derive(Debug, Clone)]
pub struct PalettedBitBuffer {
    data: BitBuffer,
    palette: Vec<u32>,
    max_entries: u32,
    use_palette: bool,
    /// 9 for block states, 4 for biomes
    direct_threshold: u64,
}

impl PalettedBitBuffer {
    pub fn new(entries: usize, direct_threshold: u64) -> PalettedBitBuffer {
        let palette = vec![0];
        PalettedBitBuffer {
            data: BitBuffer::create(4, entries),
            palette,
            max_entries: 16,
            use_palette: true,
            direct_threshold,
        }
    }

    fn load(
        entries: usize,
        bits_per_entry: u8,
        longs: Vec<u64>,
        palette: Vec<u32>,
        direct_threshold: u64,
    ) -> PalettedBitBuffer {
        PalettedBitBuffer {
            data: BitBuffer::load(entries, bits_per_entry, longs),
            palette,
            use_palette: bits_per_entry < 9,
            max_entries: 1 << bits_per_entry,
            direct_threshold,
        }
    }

    fn resize_buffer(&mut self) {
        assert!(
            self.use_palette,
            "The buffer should never resizing if it's already using global palette"
        );
        let old_bits_per_entry = self.data.bits_per_entry;
        // It is more efficient to use the global palette when the bits reaches the treshold
        // As of 1.16, the global palette requires 15 bits
        let new_bits = if old_bits_per_entry + 1 >= self.direct_threshold {
            self.max_entries = 1 << 15;
            self.use_palette = false;
            15
        } else {
            self.max_entries <<= 1;
            old_bits_per_entry as u8 + 1
        };
        // Swap out the old buffer
        let mut old_buffer = BitBuffer::create(new_bits, self.data.entries);
        mem::swap(&mut self.data, &mut old_buffer);
        // Copy entries into new buffer
        if new_bits == 15 {
            for entry_idx in 0..old_buffer.entries {
                let entry = self.palette[old_buffer.get_entry(entry_idx) as usize];
                self.data.set_entry(entry_idx, entry);
            }
            // Deallocate the old palette
            self.palette = Vec::new();
        } else {
            for entry_idx in 0..old_buffer.entries {
                let entry = old_buffer.get_entry(entry_idx);
                self.data.set_entry(entry_idx, entry);
            }
        }
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

    fn encode_packet(&self) -> PalettedContainer {
        if self.use_palette && self.palette.len() == 1 {
            PalettedContainer {
                bits_per_entry: 0,
                data_array: vec![0],
                palette: Some(vec![self.palette[0] as i32]),
            }
        } else {
            PalettedContainer {
                bits_per_entry: self.data.bits_per_entry as u8,
                data_array: self.data.longs.clone(),
                palette: self
                    .use_palette
                    .then(|| self.palette.clone().into_iter().map(|x| x as i32).collect()),
            }
        }
    }
}

pub struct ChunkSection {
    buffer: PalettedBitBuffer,
    block_count: u32,
    multi_block: CUpdateSectionBlocks,
    changed_blocks: [i16; 16 * 16 * 16],
    changed: bool,
}

impl ChunkSection {
    fn get_index(x: u32, y: u32, z: u32) -> usize {
        ((y << 8) | (z << 4) | x) as usize
    }

    fn get_block(&self, x: u32, y: u32, z: u32) -> u32 {
        let idx = ChunkSection::get_index(x, y, z);
        if self.changed_blocks[idx] >= 0 {
            self.changed_blocks[idx] as u32
        } else {
            self.buffer.get_entry(idx)
        }
    }

    /// Sets a block in the chunk sections. Returns true if a block was changed.
    fn set_block(&mut self, x: u32, y: u32, z: u32, block: u32) -> bool {
        let old_block = self.get_block(x, y, z);
        if old_block == 0 && block != 0 {
            self.block_count += 1;
        } else if old_block != 0 && block == 0 {
            self.block_count -= 1;
        }
        let idx = ChunkSection::get_index(x, y, z);
        let changed = old_block != block;
        if changed {
            self.changed = true;
            self.changed_blocks[idx] = block as i16;
        }
        changed
    }

    fn load(data: Option<ChunkSectionData>) -> ChunkSection {
        let data = match data {
            Some(data) => data,
            None => return Default::default(),
        };

        let loaded_longs = data.data.into_iter().map(|x| x as u64).collect();
        let bits_per_entry = data.bits_per_block as u8;
        let palette = data.palette.into_iter().map(|x| x as u32).collect();
        let buffer =
            PalettedBitBuffer::load(data.entries, bits_per_entry, loaded_longs, palette, 9);
        ChunkSection {
            buffer,
            block_count: data.block_count as u32,
            multi_block: CUpdateSectionBlocks {
                chunk_x: 0,
                chunk_y: 0,
                chunk_z: 0,
                records: Vec::new(),
            },
            changed_blocks: [-1; 16 * 16 * 16],
            changed: false,
        }
    }

    fn save(&mut self) -> Option<ChunkSectionData> {
        self.flush();
        if self.buffer.use_palette && self.buffer.palette.len() == 1 && self.buffer.palette[0] == 0
        {
            // chunk section is completely air
            return None;
        }

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
        Some(ChunkSectionData {
            data: longs,
            palette,
            bits_per_block: self.buffer.data.bits_per_entry as i8,
            block_count: self.block_count as i32,
            entries: self.buffer.entries(),
        })
    }

    fn compress(&mut self) {
        let mut new_buffer = PalettedBitBuffer::new(4096, 9);
        for i in 0..4096 {
            new_buffer.set_entry(i, self.buffer.get_entry(i));
        }
        self.buffer = new_buffer;
    }

    fn encode_packet(&self) -> CChunkDataSection {
        CChunkDataSection {
            block_count: self.block_count as i16,
            block_states: self.buffer.encode_packet(),
            biomes: PalettedContainer {
                bits_per_entry: 0,
                data_array: vec![],
                palette: Some(vec![0]),
            },
        }
    }

    fn flush(&mut self) {
        if self.changed {
            for (i, block) in self.changed_blocks.iter().enumerate() {
                if *block >= 0 {
                    self.buffer.set_entry(i, *block as u32);
                }
            }
        }
    }

    fn multi_block(&mut self, chunk_x: i32, chunk_y: u32, chunk_z: i32) -> &CUpdateSectionBlocks {
        self.multi_block.chunk_x = chunk_x;
        self.multi_block.chunk_y = chunk_y;
        self.multi_block.chunk_z = chunk_z;
        if self.changed {
            for (i, block) in self.changed_blocks.iter().enumerate() {
                if *block >= 0 {
                    self.buffer.set_entry(i, *block as u32);
                    self.multi_block.records.push(CUpdateSectionBlocksRecord {
                        block_id: *block as u32,
                        x: (i & 0xF) as u8,
                        y: (i >> 8) as u8,
                        z: ((i & 0xF0) >> 4) as u8,
                    });
                }
            }
            self.changed = false;
            self.changed_blocks = [-1; 16 * 16 * 16];
        }
        &self.multi_block
    }

    pub fn block_count(&self) -> u32 {
        self.block_count
    }
}

impl Default for ChunkSection {
    fn default() -> ChunkSection {
        ChunkSection {
            buffer: PalettedBitBuffer::new(4096, 9),
            block_count: 0,
            multi_block: CUpdateSectionBlocks {
                chunk_x: 0,
                chunk_y: 0,
                chunk_z: 0,
                records: Vec::new(),
            },
            changed_blocks: [-1; 16 * 16 * 16],
            changed: false,
        }
    }
}

pub struct Chunk {
    pub sections: [ChunkSection; PLOT_SECTIONS],
    pub x: i32,
    pub z: i32,
    pub block_entities: FxHashMap<BlockPos, BlockEntity>,
}

impl Chunk {
    pub fn encode_packet(&self) -> PacketEncoder {
        // Integer arithmetic trick: ceil(log2(x)) can be calculated with 32 - (x - 1).leading_zeros().
        // See also: https://wiki.vg/Protocol#Chunk_Data_and_Update_Light
        const HEIGHTMAP_BITS: u8 =
            (32 - ((PLOT_BLOCK_HEIGHT as u32 + 1) - 1).leading_zeros()) as u8;
        let mut heightmap_buffer = BitBuffer::create(HEIGHTMAP_BITS, 16 * 16);
        for x in 0..16 {
            for z in 0..16 {
                heightmap_buffer
                    .set_entry((x * 16) + z, self.get_top_most_block(x as u32, z as u32));
            }
        }

        let mut chunk_sections = Vec::new();
        for section in &self.sections {
            chunk_sections.push(section.encode_packet());
        }
        let mut heightmaps = nbt::Map::new();
        let heightmap_longs: Vec<i64> = heightmap_buffer
            .longs
            .into_iter()
            .map(|x| x as i64)
            .collect();
        heightmaps.insert(
            "MOTION_BLOCKING".to_string(),
            nbt::Value::LongArray(heightmap_longs),
        );
        let mut block_entities = Vec::new();
        for (pos, block_entity) in &self.block_entities {
            if let Some(nbt) = block_entity.to_nbt(true) {
                block_entities.push(CChunkDataBlockEntity {
                    x: pos.x as i8,
                    z: pos.z as i8,
                    y: pos.y as i16,
                    data: nbt.content,
                    ty: block_entity.ty(),
                })
            }
        }
        CChunkData {
            chunk_sections,
            chunk_x: self.x,
            chunk_z: self.z,
            heightmaps,
            block_entities,
        }
        .encode()
    }

    pub fn encode_empty_packet(x: i32, z: i32) -> PacketEncoder {
        CChunkData {
            chunk_sections: (0..PLOT_SECTIONS)
                .map(|_| CChunkDataSection {
                    block_count: 0,
                    block_states: PalettedContainer {
                        bits_per_entry: 0,
                        data_array: vec![0],
                        palette: Some(vec![0]),
                    },
                    biomes: PalettedContainer {
                        bits_per_entry: 0,
                        data_array: vec![0],
                        palette: Some(vec![0]),
                    },
                })
                .collect(),
            chunk_x: x,
            chunk_z: z,
            heightmaps: nbt::Map::new(),
            block_entities: vec![],
        }
        .encode()
    }

    fn get_top_most_block(&self, x: u32, z: u32) -> u32 {
        let mut top_most = 0;
        for (section_y, section) in self.sections.iter().enumerate() {
            for y in (0..16).rev() {
                let block_state = section.get_block(x, y, z);
                if block_state != 0 && top_most < y + section_y as u32 * 16 {
                    top_most = section_y as u32 * 16;
                }
            }
        }
        top_most
    }

    /// Sets a block in the chunk. Returns true if a block was changed.
    pub fn set_block(&mut self, x: u32, y: u32, z: u32, block_id: u32) -> bool {
        let section_y = (y >> 4) as usize;
        let section = &mut self.sections[section_y];
        section.set_block(x, y & 0xF, z, block_id)
    }

    pub fn get_block(&self, x: u32, y: u32, z: u32) -> u32 {
        let section_y = (y / 16) as usize;
        match self.sections.get(section_y) {
            Some(section) => section.get_block(x, y & 0xF, z),
            None => 0,
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

    pub fn save(&mut self) -> ChunkData<PLOT_SECTIONS> {
        ChunkData {
            sections: self
                .sections
                .iter_mut()
                .map(|s| s.save())
                .collect_vec()
                .try_into()
                .unwrap(),
            block_entities: self.block_entities.clone(),
        }
    }

    pub fn load(x: i32, z: i32, chunk_data: ChunkData<PLOT_SECTIONS>) -> Chunk {
        Chunk {
            x,
            z,
            sections: IntoIterator::into_iter(chunk_data.sections)
                .map(ChunkSection::load)
                .collect_vec()
                .try_into()
                .map_err(|_| ())
                .unwrap(),
            block_entities: chunk_data.block_entities,
        }
    }

    pub fn compress(&mut self) {
        self.sections
            .iter_mut()
            .for_each(|section| section.compress());
    }

    pub fn empty(x: i32, z: i32) -> Chunk {
        Chunk {
            sections: std::array::from_fn(|_| Default::default()),
            x,
            z,
            block_entities: FxHashMap::default(),
        }
    }

    pub fn multi_blocks(&mut self) -> impl Iterator<Item = &CUpdateSectionBlocks> {
        let x = self.x;
        let z = self.z;
        self.sections
            .iter_mut()
            .enumerate()
            .filter_map(move |(y, section)| {
                section
                    .changed
                    .then(move || section.multi_block(x, y as u32, z))
            })
    }

    pub fn reset_multi_blocks(&mut self) {
        for section in &mut self.sections {
            section.multi_block.records.clear();
        }
    }
}
