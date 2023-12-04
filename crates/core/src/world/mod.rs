pub mod storage;

use mchprs_blocks::block_entities::BlockEntity;
use mchprs_blocks::blocks::Block;
use mchprs_blocks::BlockPos;
use mchprs_world::TickPriority;
use storage::Chunk;

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

// TODO: I have no idea how to deduplicate this in a sane way

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
