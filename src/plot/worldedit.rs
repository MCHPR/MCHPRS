use super::Plot;
use crate::blocks::Block;
use crate::network::packets::clientbound::*;
use std::collections::HashMap; 

pub struct MultiBlockChangeRecord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub block_id: u32
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

    pub(super) fn worldedit_set(&mut self, player: usize, block: Block) {
        if let Some((first_pos, second_pos)) = self.worldedit_verify_positions(player) {
            let mut blocks_updated = 0;
            let mut records: Vec<MultiBlockChangeRecord> = Vec::new();

            let x_start = std::cmp::min(first_pos.0, second_pos.0);
            let x_end = std::cmp::max(first_pos.0, second_pos.0);
            let y_start = std::cmp::min(first_pos.1, second_pos.1);
            let y_end = std::cmp::max(first_pos.1, second_pos.1);
            let z_start = std::cmp::min(first_pos.2, second_pos.2);
            let z_end = std::cmp::max(first_pos.2, second_pos.2);

            let block_id = block.get_id();

            for x in x_start..=x_end {
                for y in y_start..=y_end {
                    for z in z_start..=z_end {
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
}
