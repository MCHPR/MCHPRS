use mchprs_blocks::blocks::{Block, Instrument};
use mchprs_blocks::{BlockFace, BlockPos};

use crate::world::World;

// LUT generated via f32::powf(2.0, (note as f32 - 12.0) / 12.0)
// This is hardcoded because at this point floating point operations are not allowed in const contexts
const PITCHES_TABLE: [f32; 25] = [
    0.5, 0.5297315, 0.561231, 0.59460354, 0.62996054, 0.6674199, 0.70710677, 0.74915355, 0.7937005,
    0.8408964, 0.8908987, 0.9438743, 1.0, 1.0594631, 1.122462, 1.1892071, 1.2599211, 1.3348398,
    1.4142135, 1.4983071, 1.587401, 1.6817929, 1.7817974, 1.8877486, 2.0,
];

pub fn is_noteblock_unblocked(world: &impl World, pos: BlockPos) -> bool {
    matches!(world.get_block(pos.offset(BlockFace::Top)), Block::Air {})
}

pub fn get_noteblock_instrument(world: &impl World, pos: BlockPos) -> Instrument {
    Instrument::from_block_below(world.get_block(pos.offset(BlockFace::Bottom)))
}

pub fn play_note(world: &mut impl World, pos: BlockPos, instrument: Instrument, note: u32) {
    world.play_sound(
        pos,
        instrument.to_sound_id(),
        2, // Sound Caregory ID for Records
        3.0,
        PITCHES_TABLE[note as usize],
    );
}
