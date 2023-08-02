use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use mchprs_blocks::blocks::Block;

/// This build script generates a perfect hash set to filter out any unnecessary block changes.
/// Best used for big redstone screens as it eases the load on the mincraft client's chunk rendering threads, resulting in visually faster updates.
/// Toggle the filter with //toggleioonly
/// Warning: Filtering will cause redstone components to look like they aren't updating. This persists until the affected blocks are changed again or the chunk is reloaded
fn main() {
    let path = Path::new(&env::var("OUT_DIR").unwrap()).join("io_only_filter.rs");
    let mut file = BufWriter::new(File::create(&path).unwrap());

    let mut set = phf_codegen::Set::<u32>::new();

    // Magic number, not sure how many total block states there are, but 2^16 should hopefully be enough
    for id in 0..65536 {
        let block = Block::from_id(id);

        match block {
            Block::RedstoneWire { .. }
            | Block::RedstoneTorch { .. }
            | Block::RedstoneWallTorch { .. }
            | Block::RedstoneRepeater { .. }
            //| Block::RedstoneLamp { .. }
            | Block::RedstoneComparator { .. }
            | Block::Observer { .. }
            //| Block::IronTrapdoor { .. }
            => {
                set.entry(id);
            }
            _ => {}
        }
    }
    write!(
        &mut file,
        "static IO_ONLY_BLACKLIST: phf::Set<u32> = {};\n",
        set.build()
    )
    .unwrap();
}
