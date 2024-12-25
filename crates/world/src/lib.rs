pub mod storage;

use mchprs_blocks::block_entities::BlockEntity;
use mchprs_blocks::blocks::Block;
use mchprs_blocks::BlockPos;
use serde::{Deserialize, Serialize};
use storage::Chunk;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TickPriority {
    Highest = 0,
    Higher = 1,
    High = 2,
    Normal = 3,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TickEntry {
    pub ticks_left: u32,
    pub tick_priority: TickPriority,
    pub pos: BlockPos,
}

pub trait World {
    /// Returns the block located at `pos`
    fn get_block(&self, pos: BlockPos) -> Block {
        Block::from_id(self.get_block_raw(pos))
    }

    /// Returns the block state id of the block at `pos`
    fn get_block_raw(&self, pos: BlockPos) -> u32;

    /// Sets the block at `pos`.
    /// This function may have side effects such as sending update block packets to the player.
    /// Returns true if the block was changed.
    fn set_block(&mut self, pos: BlockPos, block: Block) -> bool {
        let block_id = Block::get_id(block);
        self.set_block_raw(pos, block_id)
    }

    /// Sets a block in storage without any other side effects. Returns true if a block was changed.
    fn set_block_raw(&mut self, pos: BlockPos, block: u32) -> bool;

    /// Removes a block entity at `pos` if it exists.
    fn delete_block_entity(&mut self, pos: BlockPos);

    /// Returns a reference to the block entity at `pos` if it exists.
    /// Returns None if there is no block entity at `pos`.
    fn get_block_entity(&self, pos: BlockPos) -> Option<&BlockEntity>;

    /// Sets the block entity at `pos`, overwriting any other block entity that was there prior.
    fn set_block_entity(&mut self, pos: BlockPos, block_entity: BlockEntity);

    /// Returns an immutable reference to the chunk at `x` and `z` chunk coordinates.
    /// Returns None if the chunk does not exist in this world.
    fn get_chunk(&self, x: i32, z: i32) -> Option<&Chunk>;

    /// Returns a mutable reference to the chunk at `x` and `z` chunk coordinates.
    /// Returns None if the chunk does not exist in this world.
    fn get_chunk_mut(&mut self, x: i32, z: i32) -> Option<&mut Chunk>;

    /// Schedules a tick in the world with `delay` and `pritority`
    fn schedule_tick(&mut self, pos: BlockPos, delay: u32, priority: TickPriority);

    /// Returns true if there is a tick entry with `pos`
    fn pending_tick_at(&mut self, pos: BlockPos) -> bool;

    fn is_cursed(&self) -> bool {
        false
    }

    #[allow(unused_variables)]
    fn play_sound(
        &mut self,
        pos: BlockPos,
        sound_id: i32,
        sound_category: i32,
        volume: f32,
        pitch: f32,
    ) {
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ChunkSectionIdx {
    x: i32,
    y: i32,
    z: i32,
}

impl ChunkSectionIdx {
    fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }
}

/// Returns an iterator over the chunk section indexes between two block positions,
/// ie, over x,y,z triples (where x,z are chunk indexes and y are section indexes)
/// such that the bounding box defined by the two block positions intersect
/// section y of the x,z chunk.
/// The iterator yields the chunks in x,z order, and the sections in y order,
/// i.e., if visiting sections 1,2 of chunks 0,0 and 0,1, the iterator will yield
/// - section 1 of chunk 0,0 (0,1,0)
/// - section 2 of chunk 0,0 (0,1,1)
/// - section 1 of chunk 0,1 (0,2,0)
/// - section 2 of chunk 0,1 (0,2,1)
/// The iterator will not yield any chunks or sections
/// that are entirely outside the bounding box.
fn chunk_section_idxs_between(
    fst: BlockPos,
    snd: BlockPos,
) -> impl Iterator<Item = ChunkSectionIdx> {
    // for each coordinate, we find the start and end value based on the relative
    // position of the two blocks, and then divide by 16 to get the chunk/section index
    let start_x = i32::min(fst.x, snd.x).div_euclid(16);
    let end_x = i32::max(fst.x, snd.x).div_euclid(16);

    let start_y = i32::min(fst.y, snd.y).div_euclid(16);
    let end_y = i32::max(fst.y, snd.y).div_euclid(16);

    let start_z = i32::min(fst.z, snd.z).div_euclid(16);
    let end_z = i32::max(fst.z, snd.z).div_euclid(16);

    // it's possible that putting y in the final flat_map would let the compiler optimise the
    // iterations better (after inlining the iterator loop) if we then do operations
    // on the chunk that don't depend on the sections... should be tested
    (start_x..=end_x)
        .flat_map(move |x| (start_z..=end_z).map(move |z| (x, z)))
        .flat_map(move |(x, z)| (start_y..=end_y).map(move |y| ChunkSectionIdx::new(x, y, z)))
}

/// Returns an iterator over the block positions contained in the given chunk section
/// and within the bounding box defined by the two block positions.
fn block_pos_in_chunk_section_between(
    fst: BlockPos,
    snd: BlockPos,
    chunk_section_idx: ChunkSectionIdx,
) -> impl Iterator<Item = BlockPos> {
    // for each coordinate, we start at either the lowest coordinate of the bounding
    // blocks or at the chunk boundary if the chunk is contained in the bounding box
    // and correspondingly end at the highest coordinate of the bounding blocks or at the
    // chunk boundary if the chunk is contained in the bounding box
    let start_x = i32::min(fst.x, snd.x).max(chunk_section_idx.x * 16);
    let end_x = i32::max(fst.x, snd.x).min(chunk_section_idx.x * 16 + 15);

    let start_y = i32::min(fst.y, snd.y).max(chunk_section_idx.y * 16);
    let end_y = i32::max(fst.y, snd.y).min(chunk_section_idx.y * 16 + 15);

    let start_z = i32::min(fst.z, snd.z).max(chunk_section_idx.z * 16);
    let end_z = i32::max(fst.z, snd.z).min(chunk_section_idx.z * 16 + 15);

    (start_x..=end_x)
        .flat_map(move |x| (start_y..=end_y).map(move |y| (x, y)))
        .flat_map(move |(x, y)| (start_z..=end_z).map(move |z| BlockPos::new(x, y, z)))
}

/// Executes the given function for each block excluding most air blocks
pub fn for_each_block_optimized<F, W: World>(
    world: &W,
    first_pos: BlockPos,
    second_pos: BlockPos,
    mut f: F,
) where
    F: FnMut(BlockPos),
{
    let start_x = i32::min(first_pos.x, second_pos.x);
    let end_x = i32::max(first_pos.x, second_pos.x);

    let start_y = i32::min(first_pos.y, second_pos.y);
    let end_y = i32::max(first_pos.y, second_pos.y);

    let start_z = i32::min(first_pos.z, second_pos.z);
    let end_z = i32::max(first_pos.z, second_pos.z);

    // Iterate over chunks
    for chunk_start_x in (start_x..=end_x).step_by(16) {
        for chunk_start_z in (start_z..=end_z).step_by(16) {
            let chunk = world
                .get_chunk(chunk_start_x.div_euclid(16), chunk_start_z.div_euclid(16))
                .unwrap();
            for chunk_start_y in (start_y..=end_y).step_by(16) {
                // Check if the chunk even has non air blocks
                if chunk.sections[chunk_start_y as usize / 16].block_count() > 0 {
                    // Calculate the end position of the current chunk
                    let chunk_end_x = i32::min(chunk_start_x + 16 - 1, end_x);
                    let chunk_end_y = i32::min(chunk_start_y + 16 - 1, end_y);
                    let chunk_end_z = i32::min(chunk_start_z + 16 - 1, end_z);

                    // Iterate over each position within the current chunk
                    for y in chunk_start_y..=chunk_end_y {
                        for z in chunk_start_z..=chunk_end_z {
                            for x in chunk_start_x..=chunk_end_x {
                                let pos = BlockPos::new(x, y, z);
                                f(pos);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Executes the given function for each block excluding most air blocks
pub fn for_each_block_mut_optimized<F, W: World>(
    world: &mut W,
    first_pos: BlockPos,
    second_pos: BlockPos,
    mut f: F,
) where
    F: FnMut(&mut W, BlockPos),
{
    let start_x = i32::min(first_pos.x, second_pos.x);
    let end_x = i32::max(first_pos.x, second_pos.x);

    let start_y = i32::min(first_pos.y, second_pos.y);
    let end_y = i32::max(first_pos.y, second_pos.y);

    let start_z = i32::min(first_pos.z, second_pos.z);
    let end_z = i32::max(first_pos.z, second_pos.z);

    // Iterate over chunks
    for chunk_start_x in (start_x..=end_x).step_by(16) {
        for chunk_start_z in (start_z..=end_z).step_by(16) {
            for chunk_start_y in (start_y..=end_y).step_by(16) {
                // Check if the chunk even has non air blocks
                if world
                    .get_chunk(chunk_start_x.div_euclid(16), chunk_start_z.div_euclid(16))
                    .unwrap()
                    .sections[chunk_start_y as usize / 16]
                    .block_count()
                    > 0
                {
                    // Calculate the end position of the current chunk
                    let chunk_end_x = i32::min(chunk_start_x + 16 - 1, end_x);
                    let chunk_end_y = i32::min(chunk_start_y + 16 - 1, end_y);
                    let chunk_end_z = i32::min(chunk_start_z + 16 - 1, end_z);

                    // Iterate over each position within the current chunk
                    for y in chunk_start_y..=chunk_end_y {
                        for z in chunk_start_z..=chunk_end_z {
                            for x in chunk_start_x..=chunk_end_x {
                                let pos = BlockPos::new(x, y, z);
                                f(world, pos);
                            }
                        }
                    }
                }
            }
        }
    }
}
