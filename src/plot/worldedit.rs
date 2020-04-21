use super::Plot;
use crate::blocks::Block;
use crate::network::packets::clientbound::*;
use std::collections::HashMap;
use rand::Rng;
use regex::{Regex,Captures};

pub struct MultiBlockChangeRecord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub block_id: u32
}

pub struct WorldEditPatternPart {
    pub weight: f32,
    pub block_id: u32
}

pub struct WorldEditPattern {
    pub parts: Vec<WorldEditPatternPart>
}

impl WorldEditPattern {
    pub fn from_str(pattern_str: &str) -> Option<WorldEditPattern> {
        let mut pattern = WorldEditPattern {
            parts: Vec::new()
        };
        
        for part in pattern_str.split(",") {
            let re = Regex::new(r"^(([0-9]+(\.[0-9]+)?)%)?(=)?([0-9]+|(minecraft:)?[a-zA-Z_]+)(:([0-9]+)|\[(([a-zA-Z_]+=[a-zA-Z0-9]+,?)+?)\])?((\|([^|]*?)){1,4})?$").unwrap();
            let pattern_match = re.captures(part);
            
            
            if let Some(pattern_match) = pattern_match {
                let block: Option<Block>;

                if pattern_match.get(4).is_some() {
                    block = Some(Block::from_block_state(
                        pattern_match
                            .get(5)
                            .map_or("0", |m| m.as_str())
                            .parse::<u32>()
                            .unwrap()
                    ));
                } else {
                    let block_name = pattern_match.get(5).unwrap().as_str();
                    block = Block::from_name(block_name)
                }

                let weight = pattern_match.get(2).map_or("100", |m| m.as_str()).parse::<f32>().unwrap() / 100.0;

                if let Some(block) = block {
                    pattern.parts.push(WorldEditPatternPart {
                        weight,
                        block_id: block.get_id()
                    });
                } else {
                    return None;
                }
            }
        }
        Some(pattern)
    }

    pub fn matches(&self, block: Block) -> bool {
        for part in &self.parts {
            if block.get_id() == part.block_id {
                return true;
            }
        }
        false
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
            weight: 0.0
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

impl Plot {
    fn worldedit_multi_block_change(&mut self, records: &Vec<MultiBlockChangeRecord>) {
        let mut packets: HashMap<usize, C10MultiBlockChange> = HashMap::new();
        for record in records {
            let chunk_index = Plot::get_chunk_index(record.x, record.z);

            packets.entry(chunk_index).or_insert(C10MultiBlockChange {
                chunk_x: record.x >> 4,
                chunk_z: record.z >> 4,
                records: Vec::new()
            }).records.push(C10MultiBlockChangeRecord {
                block_id: record.block_id as i32,
                x: (record.x % 16) as i8,
                y: record.y as u8,
                z: (record.z % 16) as i8,
            });
        }

        for (_, packet) in packets {
            let multi_block_change = packet.encode();

            for player in &mut self.players {
                player.client.send_packet(&multi_block_change);
            }
        }
    }

    fn worldedit_verify_positions(
        &mut self,
        player: usize,
    ) -> Option<((i32, i32, i32), (i32, i32, i32))> {
        let player = &mut self.players[player];
        let first_pos;
        let second_pos;
        if let Some(pos) = player.first_position {
            first_pos = pos;
        } else {
            player.send_system_message("First position is not set!");
            return None;
        }
        if let Some(pos) = player.second_position {
            second_pos = pos;
        } else {
            player.send_system_message("Second position is not set!");
            return None;
        }
        if !Plot::in_plot_bounds(self.x, self.z, first_pos.0, first_pos.2) {
            player.send_system_message("First position is outside plot bounds!");
            return None;
        }
        if !Plot::in_plot_bounds(self.x, self.z, first_pos.0, first_pos.2) {
            player.send_system_message("Second position is outside plot bounds!");
            return None;
        }
        Some((first_pos, second_pos))
    }

    pub(super) fn worldedit_set(&mut self, player: usize, pattern: WorldEditPattern) {
        if let Some((first_pos, second_pos)) = self.worldedit_verify_positions(player) {
            let mut blocks_updated = 0;
            let mut records: Vec<MultiBlockChangeRecord> = Vec::new();

            let x_start = std::cmp::min(first_pos.0, second_pos.0);
            let x_end = std::cmp::max(first_pos.0, second_pos.0);
            let y_start = std::cmp::min(first_pos.1, second_pos.1);
            let y_end = std::cmp::max(first_pos.1, second_pos.1);
            let z_start = std::cmp::min(first_pos.2, second_pos.2);
            let z_end = std::cmp::max(first_pos.2, second_pos.2);

            for x in x_start..=x_end {
                for y in y_start..=y_end {
                    for z in z_start..=z_end {
                        let block_id = pattern.pick().get_id();
                        records.push(MultiBlockChangeRecord {
                            x,
                            y,
                            z,
                            block_id
                        });
                        if self.set_block_raw(x, y as u32, z, block_id) {
                            blocks_updated += 1;
                        }
                    }
                }
            }
            self.worldedit_multi_block_change(&records);
            self.players[player].worldedit_send_message(format!(
                "Operation completed: {} block(s) affected",
                blocks_updated
            ));
        }
    }

    pub(super) fn worldedit_replace(&mut self, player: usize, filter: WorldEditPattern, pattern: WorldEditPattern) {
        if let Some((first_pos, second_pos)) = self.worldedit_verify_positions(player) {
            let mut blocks_updated = 0;
            let mut records: Vec<MultiBlockChangeRecord> = Vec::new();

            let x_start = std::cmp::min(first_pos.0, second_pos.0);
            let x_end = std::cmp::max(first_pos.0, second_pos.0);
            let y_start = std::cmp::min(first_pos.1, second_pos.1);
            let y_end = std::cmp::max(first_pos.1, second_pos.1);
            let z_start = std::cmp::min(first_pos.2, second_pos.2);
            let z_end = std::cmp::max(first_pos.2, second_pos.2);

            for x in x_start..=x_end {
                for y in y_start..=y_end {
                    for z in z_start..=z_end {
                        if filter.matches(self.get_block(x, y as u32, z)) {
                            let block_id = pattern.pick().get_id();

                            records.push(MultiBlockChangeRecord {
                                x,
                                y,
                                z,
                                block_id
                            });
                            if self.set_block_raw(x, y as u32, z, block_id) {
                                blocks_updated += 1;
                            }
                        }
                    }
                }
            }
            self.worldedit_multi_block_change(&records);
            self.players[player].worldedit_send_message(format!(
                "Operation completed: {} block(s) affected",
                blocks_updated
            ));
        }
    }
}
