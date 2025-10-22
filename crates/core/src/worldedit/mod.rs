pub(crate) mod schematic;

use crate::{player::PlayerPos, plot::PlotWorld};
use mchprs_blocks::{block_entities::BlockEntity, blocks::Block, BlockPos};
use mchprs_world::{for_each_block_mut_optimized, storage::PalettedBitBuffer, World};
use once_cell::sync::Lazy;
use rand::Rng;
use regex::Regex;
use rustc_hash::FxHashMap;
use std::{fmt, str::FromStr};

#[derive(Clone, Debug)]
pub struct WorldEditClipboard {
    pub offset_x: i32,
    pub offset_y: i32,
    pub offset_z: i32,
    pub size_x: u32,
    pub size_y: u32,
    pub size_z: u32,
    pub data: PalettedBitBuffer,
    pub block_entities: FxHashMap<BlockPos, BlockEntity>,
}

#[derive(Clone, Debug)]
pub struct WorldEditUndo {
    pub clipboards: Vec<WorldEditClipboard>,
    pub pos: BlockPos,
    pub plot_x: i32,
    pub plot_z: i32,
}

#[derive(Debug, Clone)]
pub struct WorldEditPatternPart {
    pub weight: f32,
    pub block_id: u32,
}

#[derive(Clone, Debug)]
pub struct WorldEditPattern {
    pub parts: Vec<WorldEditPatternPart>,
}

pub enum PatternParseError {
    UnknownBlock(String),
    InvalidPattern(String),
}

impl fmt::Display for PatternParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PatternParseError::UnknownBlock(block) => write!(f, "unknown block: {}", block),
            PatternParseError::InvalidPattern(pattern) => write!(f, "invalid pattern: {}", pattern),
        }
    }
}

pub type PatternParseResult<T> = std::result::Result<T, PatternParseError>;

impl FromStr for WorldEditPattern {
    type Err = PatternParseError;

    fn from_str(pattern_str: &str) -> PatternParseResult<WorldEditPattern> {
        let mut pattern = WorldEditPattern { parts: Vec::new() };
        for part in pattern_str.split(',') {
            static RE: Lazy<Regex> = Lazy::new(|| {
                Regex::new(r"^(([0-9]+(\.[0-9]+)?)%)?(=)?([0-9]+|(minecraft:)?[a-zA-Z_]+)(:([0-9]+)|\[(([a-zA-Z_]+=[a-zA-Z0-9]+,?)+?)\])?((\|([^|]*?)){1,4})?$").unwrap()
            });

            let pattern_match = RE
                .captures(part)
                .ok_or_else(|| PatternParseError::InvalidPattern(part.to_owned()))?;

            let block = if pattern_match.get(4).is_some() {
                Block::from_id(
                    pattern_match
                        .get(5)
                        .map_or("0", |m| m.as_str())
                        .parse::<u32>()
                        .unwrap(),
                )
            } else {
                let block_name = pattern_match
                    .get(5)
                    .unwrap()
                    .as_str()
                    .trim_start_matches("minecraft:");
                Block::from_name(block_name)
                    .ok_or_else(|| PatternParseError::UnknownBlock(part.to_owned()))?
            };

            let weight = pattern_match
                .get(2)
                .map_or("100", |m| m.as_str())
                .parse::<f32>()
                .unwrap()
                / 100.0;

            pattern.parts.push(WorldEditPatternPart {
                weight,
                block_id: block.get_id(),
            });
        }

        Ok(pattern)
    }
}

impl WorldEditPattern {
    pub fn matches(&self, block: Block) -> bool {
        let block_id = block.get_id();
        self.parts.iter().any(|part| part.block_id == block_id)
    }

    pub fn pick(&self) -> Block {
        let mut weight_sum = 0.0;
        for part in &self.parts {
            weight_sum += part.weight;
        }

        let mut rng = rand::rng();
        let mut random = rng.random_range(0.0..weight_sum);

        let mut selected = &WorldEditPatternPart {
            block_id: 0,
            weight: 0.0,
        };

        for part in &self.parts {
            random -= part.weight;
            if random <= 0.0 {
                selected = part;
                break;
            }
        }

        Block::from_id(selected.block_id)
    }
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

        if !matches!(block, Block::Air {}) {
            return Some(block_pos);
        }

        pos.x += offset_x;
        pos.y += offset_y;
        pos.z += offset_z;
        current_distance += check_distance;
    }

    None
}

pub fn create_clipboard(
    plot: &mut PlotWorld,
    origin: BlockPos,
    first_pos: BlockPos,
    second_pos: BlockPos,
) -> WorldEditClipboard {
    let start_pos = first_pos.min(second_pos);
    let end_pos = first_pos.max(second_pos);
    let size_x = (end_pos.x - start_pos.x) as u32 + 1;
    let size_y = (end_pos.y - start_pos.y) as u32 + 1;
    let size_z = (end_pos.z - start_pos.z) as u32 + 1;
    let offset = origin - start_pos;
    let mut cb = WorldEditClipboard {
        offset_x: offset.x,
        offset_y: offset.y,
        offset_z: offset.z,
        size_x,
        size_y,
        size_z,
        data: PalettedBitBuffer::new((size_x * size_y * size_z) as usize, 9),
        block_entities: FxHashMap::default(),
    };
    let mut i = 0;
    for y in start_pos.y..=end_pos.y {
        for z in start_pos.z..=end_pos.z {
            for x in start_pos.x..=end_pos.x {
                let pos = BlockPos::new(x, y, z);
                let id = plot.get_block_raw(pos);
                let block = plot.get_block(BlockPos::new(x, y, z));
                if block.has_block_entity() {
                    if let Some(block_entity) = plot.get_block_entity(pos) {
                        cb.block_entities
                            .insert(pos - start_pos, block_entity.clone());
                    }
                }
                cb.data.set_entry(i, id);
                i += 1;
            }
        }
    }
    cb
}

pub fn clear_area(plot: &mut PlotWorld, first_pos: BlockPos, second_pos: BlockPos) {
    for_each_block_mut_optimized(plot, first_pos, second_pos, |world, pos| {
        world.set_block_raw(pos, 0);
    });

    // Send modified chunks
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

pub fn paste_clipboard(
    plot: &mut PlotWorld,
    cb: &WorldEditClipboard,
    pos: BlockPos,
    ignore_air: bool,
) {
    let offset_x = pos.x - cb.offset_x;
    let offset_y = pos.y - cb.offset_y;
    let offset_z = pos.z - cb.offset_z;
    let mut i = 0;
    // This can be made better, but right now it's not D:
    let x_range = offset_x..offset_x + cb.size_x as i32;
    let y_range = offset_y..offset_y + cb.size_y as i32;
    let z_range = offset_z..offset_z + cb.size_z as i32;

    let entries = cb.data.entries();
    // I have no clue if these clones are going to cost anything noticeable.
    'top_loop: for y in y_range {
        for z in z_range.clone() {
            for x in x_range.clone() {
                if i >= entries {
                    break 'top_loop;
                }
                let entry = cb.data.get_entry(i);
                i += 1;
                if ignore_air && entry == 0 {
                    continue;
                }
                plot.set_block_raw(BlockPos::new(x, y, z), entry);
            }
        }
    }

    // Send block changes before we send block entity data, otherwise it'll be ignored
    plot.flush_block_changes();

    for (pos, block_entity) in &cb.block_entities {
        let new_pos = BlockPos {
            x: pos.x + offset_x,
            y: pos.y + offset_y,
            z: pos.z + offset_z,
        };
        plot.set_block_entity(new_pos, block_entity.clone());
    }
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

    fn get_pos_axis(pos: &mut BlockPos, axis: u8) -> &mut i32 {
        match axis {
            0 => &mut pos.x,
            1 => &mut pos.y,
            2 => &mut pos.z,
            _ => unreachable!(),
        }
    }

    let mut expand_axis = |axis: u8| {
        let amount = *get_pos_axis(&mut amount.clone(), axis);
        let p1 = get_pos_axis(&mut p1, axis);
        let p2 = get_pos_axis(&mut p2, axis);
        #[allow(clippy::comparison_chain)]
        if amount > 0 {
            if (p1 > p2) ^ contract {
                *p1 += amount;
            } else {
                *p2 += amount;
            }
        } else if amount < 0 {
            if (p1 < p2) ^ contract {
                *p1 += amount;
            } else {
                *p2 += amount;
            }
        }
    };

    for axis in 0..=2 {
        expand_axis(axis);
    }

    (p1, p2)
}

pub fn update(plot: &mut PlotWorld, first_pos: BlockPos, second_pos: BlockPos) {
    for_each_block_mut_optimized(plot, first_pos, second_pos, |plot, pos| {
        let block = plot.get_block(pos);
        mchprs_redstone::update(block, plot, pos);
    });
}
