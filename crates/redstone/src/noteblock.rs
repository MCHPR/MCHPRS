use mchprs_blocks::blocks::{Block, Instrument};
use mchprs_blocks::{BlockFace, BlockPos};
use mchprs_world::World;

pub fn is_noteblock_unblocked(world: &impl World, pos: BlockPos) -> bool {
    matches!(world.get_block(pos.offset(BlockFace::Top)), Block::Air {})
}

pub fn get_noteblock_instrument(world: &impl World, pos: BlockPos) -> Instrument {
    Instrument::from_block_below(world.get_block(pos.offset(BlockFace::Bottom)))
}

pub fn play_note(world: &mut impl World, pos: BlockPos, instrument: Instrument, note: u32) {
    let sound_category = 2; // Sound Caregory ID for Records
    let volume = 3.0;
    // The note is mapped to [0, 31] to avoid ultra high pitches in case of invalid values.
    // The range [0, 31] is used even though it is different from the noteblock's note range
    // of [0, 24] because mapping to [0, 31] can be done efficiently using bitwise AND.
    let pitch = f32::exp2(((note % 32) as f32 - 12.0) / 12.0);
    world.play_sound(pos, instrument.to_sound_id(), sound_category, volume, pitch);
}
