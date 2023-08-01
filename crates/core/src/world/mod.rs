pub mod storage;

use mchprs_blocks::block_entities::BlockEntity;
use mchprs_blocks::blocks::Block;
use mchprs_blocks::BlockPos;
use mchprs_world::TickPriority;
use storage::Chunk;

pub trait World {
    /// Returns the block located at `pos`
    fn get_block(&self, pos: BlockPos) -> Block;

    /// Returns the block state id of the block at `pos`
    fn get_block_raw(&self, pos: BlockPos) -> u32;

    /// Sets the block at `pos`.
    /// This function may have side effects such as sending update block packets to the player.
    /// Returns true if the block was changed.
    fn set_block(&mut self, pos: BlockPos, block: Block) -> bool;

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

    fn is_cursed(&self) -> bool;
}
