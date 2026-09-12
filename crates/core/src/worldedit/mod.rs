pub(crate) mod mask;
pub(crate) mod pattern;
mod suggestions;

use crate::{player::PlayerPos, plot::PlotWorld};
use mchprs_blocks::{blocks::Block, BlockPos};
pub use mchprs_schematic::{create_clipboard, paste_clipboard, WorldEditClipboard};
use mchprs_world::{for_each_block_mut_optimized, World};
use rustc_hash::FxHashMap;

#[derive(Clone, Debug)]
pub struct WorldEditUndo {
    pub clipboards: Vec<WorldEditClipboard>,
    pub pos: BlockPos,
    pub plot_x: i32,
    pub plot_z: i32,
}

fn split_top_level_commas(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, ch) in input.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&input[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&input[start..]);
    parts
}

fn parse_raw_block(input: &str) -> Result<Block, String> {
    let id = input
        .strip_prefix('=')
        .unwrap_or(input)
        .parse::<u32>()
        .map_err(|_| format!("Invalid block state ID: {input}"))?;
    let block = Block::from_id(id);
    if block.get_id() != id {
        return Err(format!("Unsupported block state ID: {id}"));
    }
    Ok(block)
}

pub(super) fn parse_block_states(state_str: &str) -> Result<FxHashMap<String, String>, String> {
    let mut states = FxHashMap::default();
    for pair in state_str.split(',') {
        let (key, value) = pair
            .split_once('=')
            .filter(|(k, v)| !k.is_empty() && !v.is_empty())
            .ok_or_else(|| format!("Invalid block state: {pair}"))?;
        if states.insert(key.to_string(), value.to_string()).is_some() {
            return Err(format!("Duplicate block property: {key}"));
        }
    }
    Ok(states)
}

fn parse_block(input: &str) -> Result<(Block, FxHashMap<String, String>), String> {
    let (name, states) = match input.split_once('[') {
        Some((name, rest)) => {
            let states_str = rest
                .strip_suffix(']')
                .ok_or_else(|| "Unclosed bracket in block states".to_owned())?;
            (name, parse_block_states(states_str)?)
        }
        None => (input, FxHashMap::default()),
    };
    if name.is_empty() {
        return Err("Missing block name".to_owned());
    }
    let name = if name.contains(':') {
        name.to_string()
    } else {
        format!("minecraft:{}", name)
    };
    let block = Block::from_name(&name).ok_or_else(|| format!("Unknown block: {name}"))?;
    validate_states(Some(block), &states)?;
    Ok((block, states))
}

fn validate_states(block: Option<Block>, states: &FxHashMap<String, String>) -> Result<(), String> {
    for (name, value) in states {
        let values = match block {
            Some(block) => block.property_values(name).ok_or_else(|| {
                format!("Block {} does not have property {name}", block.get_name())
            })?,
            None => Block::known_properties()
                .iter()
                .find(|property| property.name == name)
                .map(|property| property.values)
                .ok_or_else(|| format!("Unknown block property: {name}"))?,
        };
        if !values.contains(&value.as_str()) {
            return Err(format!("Property {name} does not accept {value}"));
        }
    }
    Ok(())
}

pub fn ray_trace_block(
    world: &impl World,
    mut pos: PlayerPos,
    start_pitch: f64,
    start_yaw: f64,
    max_distance: f64,
) -> Option<BlockPos> {
    let check_distance = 0.2;

    // Player view height
    pos.y += 1.65;
    let rot_x = (start_yaw + 90.0) % 360.0;
    let rot_y = -start_pitch;
    let h = check_distance * rot_y.to_radians().cos();

    let offset_x = h * rot_x.to_radians().cos();
    let offset_y = check_distance * rot_y.to_radians().sin();
    let offset_z = h * rot_x.to_radians().sin();

    let mut current_distance = 0.0;

    while current_distance < max_distance {
        let block_pos = pos.block_pos();
        let block = world.get_block(block_pos);

        if !matches!(block, Block::Air) {
            return Some(block_pos);
        }

        pos.x += offset_x;
        pos.y += offset_y;
        pos.z += offset_z;
        current_distance += check_distance;
    }

    None
}

pub fn clear_area(plot: &mut PlotWorld, first_pos: BlockPos, second_pos: BlockPos) {
    for_each_block_mut_optimized(plot, first_pos, second_pos, |world, pos| {
        world.set_block_raw(pos, 0);
    });

    let start_pos = first_pos.min(second_pos);
    let end_pos = first_pos.max(second_pos);
    for chunk_x in (start_pos.x >> 4)..=(end_pos.x >> 4) {
        for chunk_z in (start_pos.z >> 4)..=(end_pos.z >> 4) {
            if let Some(chunk) = plot.get_chunk(chunk_x, chunk_z) {
                let chunk_data = chunk.encode_packet();
                for player in &mut plot.packet_senders {
                    player.send_packet(&chunk_data);
                }
            }
        }
    }
}

pub fn region_positions(
    first_pos: BlockPos,
    second_pos: BlockPos,
) -> impl Iterator<Item = BlockPos> {
    let start = first_pos.min(second_pos);
    let end = first_pos.max(second_pos);
    (start.x..=end.x).flat_map(move |x| {
        (start.y..=end.y).flat_map(move |y| (start.z..=end.z).map(move |z| BlockPos::new(x, y, z)))
    })
}

pub fn calculate_selection_volume(first_pos: BlockPos, second_pos: BlockPos) -> i32 {
    let min = first_pos.min(second_pos);
    let max = first_pos.max(second_pos);
    (max.x - min.x + 1) * (max.y - min.y + 1) * (max.z - min.z + 1)
}

pub fn calculate_expanded_selection(
    first: BlockPos,
    second: BlockPos,
    amount: BlockPos,
    contract: bool,
) -> (BlockPos, BlockPos) {
    let mut p1 = first;
    let mut p2 = second;

    for (first, second, amount) in [
        (&mut p1.x, &mut p2.x, amount.x),
        (&mut p1.y, &mut p2.y, amount.y),
        (&mut p1.z, &mut p2.z, amount.z),
    ] {
        if amount == 0 {
            continue;
        }
        let first_extends = if amount > 0 {
            *first > *second
        } else {
            *first < *second
        };
        if first_extends ^ contract {
            *first += amount;
        } else {
            *second += amount;
        }
    }

    (p1, p2)
}

pub fn update(plot: &mut PlotWorld, first_pos: BlockPos, second_pos: BlockPos) {
    for_each_block_mut_optimized(plot, first_pos, second_pos, |plot, pos| {
        let block = plot.get_block(pos);
        mchprs_redstone::update(block, plot, pos);
    });
}
