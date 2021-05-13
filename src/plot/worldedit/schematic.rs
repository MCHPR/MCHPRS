use super::WorldEditClipboard;
use crate::blocks::{Block, BlockEntity, BlockPos};
use crate::world::storage::PalettedBitBuffer;
use regex::Regex;
use std::collections::HashMap;
use std::fs::File;

pub fn load_schematic(file_name: &str) -> Option<WorldEditClipboard> {
    use nbt::Value;

    let mut file = File::open("./schems/".to_owned() + file_name).ok()?;
    let nbt = nbt::Blob::from_gzip_reader(&mut file).ok()?;
    let size_x = nbt_unwrap_val!(nbt["Width"], Value::Short) as u32;
    let size_z = nbt_unwrap_val!(nbt["Length"], Value::Short) as u32;
    let size_y = nbt_unwrap_val!(nbt["Height"], Value::Short) as u32;
    let nbt_palette = nbt_unwrap_val!(&nbt["Palette"], Value::Compound);
    let metadata = nbt_unwrap_val!(&nbt["Metadata"], Value::Compound);
    let offset_x = -nbt_unwrap_val!(metadata["WEOffsetX"], Value::Int);
    let offset_y = -nbt_unwrap_val!(metadata["WEOffsetY"], Value::Int);
    let offset_z = -nbt_unwrap_val!(metadata["WEOffsetZ"], Value::Int);
    lazy_static! {
        static ref RE: Regex = Regex::new(r"minecraft:([a-z_]+)(?:\[([a-z=,0-9]+)\])?").unwrap();
    }
    let mut palette: HashMap<u32, u32> = HashMap::new();
    for (k, v) in nbt_palette {
        let id = *nbt_unwrap_val!(v, Value::Int) as u32;
        let captures = RE.captures(&k)?;
        let mut block = Block::from_name(captures.get(1)?.as_str()).unwrap_or(Block::Air {});
        if let Some(properties_match) = captures.get(2) {
            let properties: Vec<&str> = properties_match.as_str().split(&[',', '='][..]).collect();
            for prop_idx in (0..properties.len()).step_by(2) {
                block.set_property(properties[prop_idx], properties[prop_idx + 1]);
            }
        }
        palette.insert(id, block.get_id());
    }
    let blocks: Vec<u8> = nbt_unwrap_val!(&nbt["BlockData"], Value::ByteArray)
        .iter()
        .map(|b| *b as u8)
        .collect();
    let mut data = PalettedBitBuffer::with_entries((size_x * size_y * size_z) as usize);
    let mut i = 0;
    for y_offset in (0..size_y).map(|y| y * size_z * size_x) {
        for z_offset in (0..size_z).map(|z| z * size_x) {
            for x in 0..size_x {
                let mut blockstate_id = 0;
                // Max varint length is 5
                for varint_len in 0..=5 {
                    blockstate_id |= ((blocks[i] & 127) as u32) << (varint_len * 7);
                    if (blocks[i] & 128) != 128 {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                let entry = *palette.get(&blockstate_id).unwrap();
                data.set_entry((y_offset + z_offset + x) as usize, entry);
            }
        }
    }
    let block_entities = nbt_unwrap_val!(&nbt["BlockEntities"], Value::List);
    let mut parsed_block_entities = HashMap::new();
    for block_entity in block_entities {
        let val = nbt_unwrap_val!(block_entity, Value::Compound);
        let pos_array = nbt_unwrap_val!(&val["Pos"], Value::IntArray);
        let pos = BlockPos {
            x: pos_array[0],
            y: pos_array[1],
            z: pos_array[2],
        };
        if let Some(parsed) = BlockEntity::from_nbt(val) {
            parsed_block_entities.insert(pos, parsed);
        }
    }
    Some(WorldEditClipboard {
        size_x,
        size_y,
        size_z,
        offset_x,
        offset_y,
        offset_z,
        data,
        block_entities: parsed_block_entities,
    })
}
