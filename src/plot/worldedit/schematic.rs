//! This implements Sponge Schematic Specification ver. 2
//! https://github.com/SpongePowered/Schematic-Specification/blob/master/versions/schematic-2.md

use super::WorldEditClipboard;
use crate::blocks::{Block, BlockEntity, BlockPos};
use crate::server::MC_DATA_VERSION;
use crate::world::storage::PalettedBitBuffer;
use anyhow::Result;
use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::lazy::SyncLazy;

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
    static RE: SyncLazy<Regex> =
        SyncLazy::new(|| Regex::new(r"minecraft:([a-z_]+)(?:\[([a-z=,0-9]+)\])?").unwrap());
    let mut palette: HashMap<u32, u32> = HashMap::new();
    for (k, v) in nbt_palette {
        let id = *nbt_unwrap_val!(v, Value::Int) as u32;
        let captures = RE.captures(k)?;
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

#[derive(Serialize)]
struct Metadata {
    #[serde(rename = "WEOffsetX")]
    offset_x: i32,
    #[serde(rename = "WEOffsetY")]
    offset_y: i32,
    #[serde(rename = "WEOffsetZ")]
    offset_z: i32,
}

/// Used to serialize schematics in NBT. This cannot be used for deserialization because of
/// [a bug](https://github.com/PistonDevelopers/hematite_nbt/issues/45) in `hematite-nbt`.
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Schematic {
    width: i16,
    length: i16,
    height: i16,
    palette: nbt::Blob,
    metadata: Metadata,
    #[serde(serialize_with = "nbt::i8_array")]
    block_data: Vec<i8>,
    block_entities: Vec<nbt::Blob>,
    version: i32,
    data_version: i32,
}

pub fn save_schematic(file_name: &str, clipboard: &WorldEditClipboard) -> Result<()> {
    let mut file = File::create("./schems/".to_owned() + file_name)?;
    let size_x = clipboard.size_x;
    let size_y = clipboard.size_y;
    let size_z = clipboard.size_z;
    let offset_x = -clipboard.offset_x;
    let offset_y = -clipboard.offset_y;
    let offset_z = -clipboard.offset_z;
    let blocks = &clipboard.data;

    let mut data = Vec::new();
    let mut pallette = Vec::new();
    for y_offset in (0..size_y).map(|y| y * size_z * size_x) {
        for z_offset in (0..size_z).map(|z| z * size_x) {
            for x in 0..size_x {
                let entry = blocks.get_entry((y_offset + z_offset + x) as usize);
                let block = Block::from_id(entry);

                let name = format!("minecraft:{}", block.get_name());
                let props = block.properties();
                let full_name = if !props.is_empty() {
                    let props_strs: Vec<String> = props
                        .iter()
                        .map(|(name, val)| format!("{}={}", name, val))
                        .collect();
                    format!("{}[{}]", name, props_strs.join(","))
                } else {
                    name
                };
                let idx = if let Some(idx) = pallette.iter().position(|s| *s == full_name) {
                    idx
                } else {
                    let idx = pallette.len();
                    pallette.push(full_name);
                    idx
                };
                data.push(idx as i8);
            }
        }
    }

    let mut encoded_pallete = nbt::Blob::named("Palette");
    for (i, entry) in pallette.iter().enumerate() {
        encoded_pallete.insert(entry, i as i32)?;
    }

    let mut block_entities = Vec::new();
    for (pos, block_entity) in &clipboard.block_entities {
        if let Some(blob) = block_entity.to_nbt(*pos) {
            block_entities.push(blob);
        }
    }

    let metadata = Metadata {
        offset_x,
        offset_y,
        offset_z,
    };
    let schematic = Schematic {
        width: size_x as i16,
        length: size_z as i16,
        height: size_y as i16,
        block_data: data,
        block_entities,
        palette: encoded_pallete,
        metadata,
        version: 2,
        data_version: MC_DATA_VERSION,
    };
    nbt::to_gzip_writer(&mut file, &schematic, Some("Schematic"))?;

    Ok(())
}
