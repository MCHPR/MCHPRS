use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use mchprs_blocks::blocks::Block;

/// This build script generates perfect hash sets to match certain types of blocks.
/// In non standard cases you may add components to the appropiate match statements.
fn main() {
    let path = Path::new(&env::var("OUT_DIR").unwrap()).join("block_filters.rs");
    let mut file = BufWriter::new(File::create(&path).unwrap());

    let mut input_set = phf_codegen::Set::<u32>::new();
    let mut output_set = phf_codegen::Set::<u32>::new();
    let mut changing_set = phf_codegen::Set::<u32>::new();

    // Magic number, not sure how many total block states there are, but 2^16 should hopefully be enough
    for id in 0..65536 {
        let block = Block::from_id(id);

        // Matches all blocks that should be considered as input components
        match block {
            Block::Lever { .. } | Block::StoneButton { .. } | Block::StonePressurePlate { .. } => {
                input_set.entry(id);
            }
            _ => {}
        }

        // Matches all blocks that should be considered as output components
        match block {
            Block::RedstoneLamp { .. } | Block::IronTrapdoor { .. } => {
                output_set.entry(id);
            }
            _ => {}
        }

        // Matches all blocks that may change state (active redstone components)
        match block {
            Block::RedstoneWire { .. }
            | Block::Lever { .. }
            | Block::StoneButton { .. }
            | Block::RedstoneTorch { .. }
            | Block::RedstoneWallTorch { .. }
            | Block::RedstoneRepeater { .. }
            | Block::RedstoneLamp { .. }
            | Block::RedstoneComparator { .. }
            | Block::Observer { .. }
            | Block::StonePressurePlate { .. }
            | Block::IronTrapdoor { .. } => {
                changing_set.entry(id);
            }
            _ => {}
        }
    }

    write!(
        &mut file,
        "pub static INPUT_BLOCKS: phf::Set<u32> = {};\n",
        input_set.build()
    )
    .unwrap();
    write!(
        &mut file,
        "pub static OUTPUT_BLOCKS: phf::Set<u32> = {};\n",
        output_set.build()
    )
    .unwrap();
    write!(
        &mut file,
        "pub static CHANGING_BLOCKS: phf::Set<u32> = {};\n",
        changing_set.build()
    )
    .unwrap();
}
