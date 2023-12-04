use mchprs_blocks::blocks::{noteblock_note_to_pitch, Block, Instrument};
use mchprs_blocks::{BlockFace, BlockPos};

use crate::world::World;

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
        noteblock_note_to_pitch(note),
    );
}
