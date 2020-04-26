use super::Plot;
use crate::blocks::{Block, BlockPos};
use crate::network::packets::clientbound::*;
use rand::Rng;
use regex::Regex;
use std::ops::RangeInclusive;
use std::time::Instant;

pub struct MultiBlockChangeRecord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub block_id: u32,
}

pub struct WorldEditPatternPart {
    pub weight: f32,
    pub block_id: u32,
}

pub enum PatternParseError {
    UnknownBlock(String),
    InvalidPattern(String),
}

pub type PatternParseResult<T> = std::result::Result<T, PatternParseError>;

pub struct WorldEditPattern {
    pub parts: Vec<WorldEditPatternPart>,
}

impl WorldEditPattern {
    pub fn from_str(pattern_str: &str) -> PatternParseResult<WorldEditPattern> {
        let mut pattern = WorldEditPattern { parts: Vec::new() };
        for part in pattern_str.split(',') {
            lazy_static! {
                static ref RE: Regex = Regex::new(r"^(([0-9]+(\.[0-9]+)?)%)?(=)?([0-9]+|(minecraft:)?[a-zA-Z_]+)(:([0-9]+)|\[(([a-zA-Z_]+=[a-zA-Z0-9]+,?)+?)\])?((\|([^|]*?)){1,4})?$").unwrap();
            }
            let pattern_match = RE
                .captures(part)
                .ok_or(PatternParseError::InvalidPattern(part.to_owned()))?;

            let block: Block;

            if pattern_match.get(4).is_some() {
                block = Block::from_block_state(
                    pattern_match
                        .get(5)
                        .map_or("0", |m| m.as_str())
                        .parse::<u32>()
                        .unwrap(),
                );
            } else {
                let block_name = pattern_match.get(5).unwrap().as_str();
                block = Block::from_name(block_name)
                    .ok_or(PatternParseError::UnknownBlock(part.to_owned()))?;
            }

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

    pub fn matches(&self, block: Block) -> bool {
        let block_id = block.get_id();
        self.parts.iter().any(|part| part.block_id == block_id)
    }

    pub fn pick(&self) -> Block {
        let mut weight_sum = 0.0;
        for part in &self.parts {
            weight_sum += part.weight;
        }

        let mut rng = rand::thread_rng();
        let mut random = rng.gen_range(0.0, weight_sum);

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

        Block::from_block_state(selected.block_id)
    }
}

struct WorldEditOperation {
    pub records: Vec<C10MultiBlockChange>,
    x_range: RangeInclusive<i32>,
    y_range: RangeInclusive<u32>,
    z_range: RangeInclusive<i32>
}

impl WorldEditOperation {
    fn new(first_pos: &BlockPos, second_pos: &BlockPos) -> WorldEditOperation {
        let x_start = std::cmp::min(first_pos.x, second_pos.x);
        let x_end = std::cmp::max(first_pos.x, second_pos.x);

        let y_start = std::cmp::min(first_pos.y, second_pos.y);
        let y_end = std::cmp::max(first_pos.y, second_pos.y);

        let z_start = std::cmp::min(first_pos.z, second_pos.z);
        let z_end = std::cmp::max(first_pos.z, second_pos.z);

        let mut records: Vec<C10MultiBlockChange> = Vec::new();
        
        for chunk_x in (x_start >> 4)..=(x_end >> 4) {
            for chunk_z in (z_start >> 4)..=(z_end >> 4) {
                records.push(C10MultiBlockChange {
                    chunk_x,
                    chunk_z,
                    records: Vec::new(),
                });
            }
        }

        let x_range = x_start..=x_end;
        let y_range = (y_start as u32)..=(y_end as u32);
        let z_range = z_start..=z_end;
        WorldEditOperation {
            records,
            x_range,
            y_range,
            z_range
        }
    }

    fn update_block(&mut self, block_pos: &BlockPos, block_id: u32) {
        let chunk_x = block_pos.x >> 4;
        let chunk_z = block_pos.z >> 4;

        if let Some(packet) = self.records.iter_mut().find(|c| c.chunk_x == chunk_x && c.chunk_z == chunk_z) {
            packet.records.push(C10MultiBlockChangeRecord {
                x: (block_pos.x >> 4) as i8,
                y: (block_pos.y >> 4) as u8,
                z: (block_pos.z >> 4) as i8,
                block_id: block_id as i32,
            })
        }
    }

    fn blocks_updated(&self) -> usize {
        let mut blocks_updated = 0;

        for record in &self.records {
            blocks_updated += record.records.len()
        }

        blocks_updated
    }

    fn x_range(&self) -> RangeInclusive<i32> {
        (&self.x_range).to_owned()
    }
    fn y_range(&self) -> RangeInclusive<u32> {
        (&self.y_range).to_owned()
    }
    fn z_range(&self) -> RangeInclusive<i32> {
        (&self.z_range).to_owned()
    }
}

impl Plot {
    fn worldedit_send_operation(&mut self, operation: WorldEditOperation) {
        for packet in operation.records {
            dbg!(packet.records.len());

            // if packet.records.len() >= 8192 {
                let chunk_index = self.get_chunk_index_for_chunk(packet.chunk_x, packet.chunk_z);
                dbg!(packet.chunk_x, packet.chunk_z);
                let chunk = &self.chunks[chunk_index];
                let chunk_data = chunk.encode_packet(false);
                for player in &mut self.players {
                    player.client.send_packet(&chunk_data);
                }
            // } else {
            //     let multi_block_change = &packet.encode();

            //     for player in &mut self.players {
            //         player.client.send_packet(&multi_block_change);
            //     }
            // }
        }
    }

    fn worldedit_start_operation(&mut self, player: usize) -> Option<WorldEditOperation> {
        let player = &mut self.players[player];
        let first_pos;
        let second_pos;
        if let Some(pos) = player.first_position.clone() {
            first_pos = pos;
        } else {
            player.send_system_message("First position is not set!");
            return None;
        }
        if let Some(pos) = player.second_position.clone() {
            second_pos = pos;
        } else {
            player.send_system_message("Second position is not set!");
            return None;
        }
        if !Plot::in_plot_bounds(self.x, self.z, first_pos.x, first_pos.z) {
            player.send_system_message("First position is outside plot bounds!");
            return None;
        }
        if !Plot::in_plot_bounds(self.x, self.z, first_pos.x, first_pos.z) {
            player.send_system_message("Second position is outside plot bounds!");
            return None;
        }

        Some(WorldEditOperation::new(&first_pos, &second_pos))
    }

    pub(super) fn worldedit_set(
        &mut self,
        player: usize,
        pattern_str: &str,
    ) -> PatternParseResult<()> {
        let start_time = Instant::now();
        let pattern = WorldEditPattern::from_str(pattern_str)?;

        if let Some(mut operation) = self.worldedit_start_operation(player) {
            for x in operation.x_range() {
                for y in operation.y_range() {
                    for z in operation.z_range() {
                        let block_pos = BlockPos::new(x, y as u32, z);
                        let block_id = pattern.pick().get_id();

                        if self.set_block_raw(&block_pos, block_id) {
                            operation.update_block(&block_pos, block_id);
                        }
                    }
                }
            }

            let blocks_updated = operation.blocks_updated();
            self.worldedit_send_operation(operation);

            self.players[player].worldedit_send_message(format!(
                "Operation completed: {} block(s) affected ({:?})",
                blocks_updated,
                start_time.elapsed()
            ));
        }
        Ok(())
    }

    pub(super) fn worldedit_replace(
        &mut self,
        player: usize,
        filter_str: &str,
        pattern_str: &str,
    ) -> PatternParseResult<()> {
        let start_time = Instant::now();

        let filter = WorldEditPattern::from_str(filter_str)?;
        let pattern = WorldEditPattern::from_str(pattern_str)?;

        if let Some(mut operation) = self.worldedit_start_operation(player) {
            for x in operation.x_range() {
                for y in operation.y_range() {
                    for z in operation.z_range() {
                        let block_pos = BlockPos::new(x, y as u32, z);

                        if filter.matches(self.get_block(&block_pos)) {
                            let block_id = pattern.pick().get_id();

                            if self.set_block_raw(&block_pos, block_id) {
                                operation.update_block(&block_pos, block_id);
                            }
                        }
                    }
                }
            }

            let blocks_updated = operation.blocks_updated();
            self.worldedit_send_operation(operation);

            self.players[player].worldedit_send_message(format!(
                "Operation completed: {} block(s) affected ({:?})",
                blocks_updated,
                start_time.elapsed()
            ));
        }
        Ok(())
    }

    pub(super) fn worldedit_count(
        &mut self,
        player: usize,
        filter_str: &str,
    ) -> PatternParseResult<()> {
        let start_time = Instant::now();

        let filter = WorldEditPattern::from_str(filter_str)?;

        if let Some(operation) = self.worldedit_start_operation(player) {
            let mut blocks_counted = 0;

            for x in operation.x_range() {
                for y in operation.y_range() {
                    for z in operation.z_range() {
                        let block_pos = BlockPos::new(x, y as u32, z);
                        if filter.matches(self.get_block(&block_pos)) {
                            blocks_counted += 1;
                        }
                    }
                }
            }

            self.players[player].worldedit_send_message(format!(
                "Counted {} block(s) ({:?})",
                blocks_counted,
                start_time.elapsed()
            ));
        }
        Ok(())
    }
}
