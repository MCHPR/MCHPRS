use super::storage::PalettedBitBuffer;
use super::Plot;
use crate::blocks::{Block, BlockEntity, BlockPos};
use crate::network::packets::clientbound::*;
use rand::Rng;
use regex::Regex;
use std::collections::HashMap;
use std::fs::File;
use std::ops::RangeInclusive;
use std::time::Instant;

// TODO: Actually use the multiblock change record.
// Right now I'm just resending the whole chunk no
// matter how big or small the operation is.
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

#[derive(Clone, Debug)]
pub struct WorldEditClipboard {
    pub offset_x: i32,
    pub offset_y: i32,
    pub offset_z: i32,
    pub size_x: u32,
    pub size_y: u32,
    pub size_z: u32,
    pub data: PalettedBitBuffer,
    pub block_entities: HashMap<BlockPos, BlockEntity>,
}

impl WorldEditClipboard {
    fn load_from_schematic(file_name: &str) -> Option<WorldEditClipboard> {
        // I greaty dislike this
        let mut file = match File::open("./schems/".to_owned() + file_name + ".schem") {
            Ok(file) => file,
            Err(_) => return None,
        };
        let nbt = match nbt::Blob::from_gzip_reader(&mut file) {
            Ok(blob) => blob,
            Err(_) => return None,
        };
        use nbt::Value;
        let size_x = nbt_unwrap_val!(nbt["Width"], Value::Short) as u32;
        let size_z = nbt_unwrap_val!(nbt["Length"], Value::Short) as u32;
        let size_y = nbt_unwrap_val!(nbt["Height"], Value::Short) as u32;
        let nbt_palette = nbt_unwrap_val!(&nbt["Palette"], Value::Compound);
        let metadata = nbt_unwrap_val!(&nbt["Metadata"], Value::Compound);
        let offset_x = -nbt_unwrap_val!(metadata["WEOffsetX"], Value::Int);
        let offset_y = -nbt_unwrap_val!(metadata["WEOffsetY"], Value::Int);
        let offset_z = -nbt_unwrap_val!(metadata["WEOffsetZ"], Value::Int);
        lazy_static! {
            static ref RE: Regex =
                Regex::new(r"minecraft:([a-z_]+)(?:\[([a-z=,0-9]+)\])?").unwrap();
        }
        let mut palette: HashMap<u32, u32> = HashMap::new();
        for (k, v) in nbt_palette {
            let id = *nbt_unwrap_val!(v, Value::Int) as u32;
            let captures = RE.captures(&k)?;
            let mut block = Block::from_name(captures.get(1)?.as_str()).unwrap_or(Block::Air);
            if let Some(properties_match) = captures.get(2) {
                let properties: Vec<&str> =
                    properties_match.as_str().split(&[',', '='][..]).collect();
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
                y: pos_array[1] as u32,
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

            let block = if pattern_match.get(4).is_some() {
                Block::from_block_state(
                    pattern_match
                        .get(5)
                        .map_or("0", |m| m.as_str())
                        .parse::<u32>()
                        .unwrap(),
                )
            } else {
                let block_name = pattern_match.get(5).unwrap().as_str();
                Block::from_name(block_name)
                    .ok_or(PatternParseError::UnknownBlock(part.to_owned()))?
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
    z_range: RangeInclusive<i32>,
}

impl WorldEditOperation {
    fn new(first_pos: BlockPos, second_pos: BlockPos) -> WorldEditOperation {
        let start_pos = first_pos.min(second_pos);
        let end_pos = first_pos.max(second_pos);

        let mut records: Vec<C10MultiBlockChange> = Vec::new();

        for chunk_x in (start_pos.x >> 4)..=(end_pos.x >> 4) {
            for chunk_z in (start_pos.z >> 4)..=(end_pos.z >> 4) {
                records.push(C10MultiBlockChange {
                    chunk_x,
                    chunk_z,
                    records: Vec::new(),
                });
            }
        }

        let x_range = start_pos.x..=end_pos.x;
        let y_range = (start_pos.y as u32)..=(end_pos.y as u32);
        let z_range = start_pos.z..=end_pos.z;
        WorldEditOperation {
            records,
            x_range,
            y_range,
            z_range,
        }
    }

    fn update_block(&mut self, block_pos: BlockPos, block_id: u32) {
        let chunk_x = block_pos.x >> 4;
        let chunk_z = block_pos.z >> 4;

        if let Some(packet) = self
            .records
            .iter_mut()
            .find(|c| c.chunk_x == chunk_x && c.chunk_z == chunk_z)
        {
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
        self.x_range.to_owned()
    }
    fn y_range(&self) -> RangeInclusive<u32> {
        self.y_range.to_owned()
    }
    fn z_range(&self) -> RangeInclusive<i32> {
        self.z_range.to_owned()
    }
}

impl Plot {
    fn worldedit_send_operation(&mut self, operation: WorldEditOperation) {
        for packet in operation.records {
            // if packet.records.len() >= 8192 {
            let chunk_index = self.get_chunk_index_for_chunk(packet.chunk_x, packet.chunk_z);
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
        if !Plot::in_plot_bounds(self.x, self.z, first_pos.x, first_pos.z) {
            player.send_system_message("First position is outside plot bounds!");
            return None;
        }
        if !Plot::in_plot_bounds(self.x, self.z, first_pos.x, first_pos.z) {
            player.send_system_message("Second position is outside plot bounds!");
            return None;
        }

        Some(WorldEditOperation::new(first_pos, second_pos))
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

                        if self.set_block_raw(block_pos, block_id) {
                            operation.update_block(block_pos, block_id);
                        }
                    }
                }
            }

            let blocks_updated = operation.blocks_updated();
            self.worldedit_send_operation(operation);

            self.players[player].send_worldedit_message(&format!(
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

                        if filter.matches(self.get_block(block_pos)) {
                            let block_id = pattern.pick().get_id();

                            if self.set_block_raw(block_pos, block_id) {
                                operation.update_block(block_pos, block_id);
                            }
                        }
                    }
                }
            }

            let blocks_updated = operation.blocks_updated();
            self.worldedit_send_operation(operation);

            self.players[player].send_worldedit_message(&format!(
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
                        if filter.matches(self.get_block(block_pos)) {
                            blocks_counted += 1;
                        }
                    }
                }
            }

            self.players[player].send_worldedit_message(&format!(
                "Counted {} block(s) ({:?})",
                blocks_counted,
                start_time.elapsed()
            ));
        }
        Ok(())
    }

    fn create_clipboard(
        &self,
        origin: BlockPos,
        first_pos: BlockPos,
        second_pos: BlockPos,
    ) -> WorldEditClipboard {
        let start_pos = first_pos.min(second_pos);
        let end_pos = first_pos.max(second_pos);
        let size_x = (end_pos.x - start_pos.x) as u32 + 1;
        let size_y = end_pos.y - start_pos.y + 1;
        let size_z = (end_pos.z - start_pos.z) as u32 + 1;
        let mut cb = WorldEditClipboard {
            offset_x: origin.x - start_pos.x,
            offset_y: origin.y as i32 - start_pos.y as i32,
            offset_z: origin.z - start_pos.z,
            size_x,
            size_y,
            size_z,
            data: PalettedBitBuffer::with_entries((size_x * size_y * size_z) as usize),
            // TODO: Get the block entities in the selection
            block_entities: HashMap::new(),
        };
        let mut i = 0;
        for y in start_pos.y..=end_pos.y {
            for z in start_pos.z..=end_pos.z {
                for x in start_pos.x..=end_pos.x {
                    cb.data
                        .set_entry(i, self.get_block_raw(BlockPos::new(x, y, z)));
                    i += 1;
                }
            }
        }
        cb
    }

    fn paste_clipboard(&mut self, cb: &WorldEditClipboard, pos: BlockPos) {
        let offset_x = pos.x - cb.offset_x;
        let offset_y = pos.y as i32 - cb.offset_y;
        let offset_z = pos.z - cb.offset_z;
        let mut i = 0;
        // This can be made better, but right now it's not D:
        let x_range = offset_x..offset_x + cb.size_x as i32;
        let y_range = offset_y..offset_y + cb.size_y as i32;
        let z_range = offset_z..offset_z + cb.size_z as i32;

        let entries = cb.data.entries();
        // I have no clue if these clones are going to cost anything noticeable.
        'top_loop: for y in y_range.clone() {
            for z in z_range.clone() {
                for x in x_range.clone() {
                    if i >= entries {
                        break 'top_loop;
                    }
                    self.set_block_raw(BlockPos::new(x, y as u32, z), cb.data.get_entry(i));
                    i += 1;
                }
            }
        }
        let chunk_x_range =
            (offset_x - (self.x << 8)) >> 4..=(offset_x + cb.size_x as i32 - (self.x << 8)) >> 4;
        let chunk_z_range =
            (offset_z - (self.z << 8)) >> 4..=(offset_z + cb.size_z as i32 - (self.z << 8)) >> 4;
        for chunk_x in chunk_x_range {
            for chunk_z in chunk_z_range.clone() {
                let chunk = &self.chunks[((chunk_x << 4) + chunk_z) as usize];
                let chunk_data = chunk.encode_packet(false);
                for player in &mut self.players {
                    player.client.send_packet(&chunk_data);
                }
            }
        }
        for (pos, block_entity) in &cb.block_entities {
            let new_pos = BlockPos {
                x: pos.x + offset_x,
                y: pos.y + offset_y as u32,
                z: pos.z + offset_z,
            };
            self.set_block_entity(new_pos, block_entity.clone());
        }
    }

    pub(super) fn worldedit_copy(&mut self, player: usize) {
        let start_time = Instant::now();

        // Start the operation just to verify the positions
        if self.worldedit_start_operation(player).is_some() {
            let origin = BlockPos::new(
                self.players[player].x.floor() as i32,
                self.players[player].y.floor() as u32,
                self.players[player].z.floor() as i32,
            );
            let clipboard = self.create_clipboard(
                origin,
                self.players[player].first_position.unwrap(),
                self.players[player].second_position.unwrap(),
            );
            self.players[player].worldedit_clipboard = Some(clipboard);

            self.players[player].send_worldedit_message(&format!(
                "Your selection was copied. ({:?})",
                start_time.elapsed()
            ));
        }
    }

    pub(super) fn worldedit_flip(&mut self, player: usize, axis: &str) {
        let start_time = Instant::now();
        
        if axis == "x" || axis == "y" || axis == "z" {
            // Start the operation just to verify the positions
            if self.players[player].worldedit_clipboard.is_some() {

                let clipboard = &self.players[player].worldedit_clipboard.as_ref().unwrap();
                let size_x = clipboard.size_x;
                let size_y = clipboard.size_y;
                let size_z = clipboard.size_z;

                let volume = size_x * size_y * size_z;
                
                let mut newcpdata = PalettedBitBuffer::with_entries((volume) as usize);

                let mut c_x = 0;
                let mut c_y = 0;
                let mut c_z = 0;

                for i in 0..volume {
                    let n_x;
                    let n_y;
                    let n_z;

                    if axis == "x" {
                        n_x = size_x-1-c_x;
                    } else {
                        n_x = c_x.clone();
                    }

                    if axis == "y" {
                        n_y = size_y-1-c_y;
                    } else {
                        n_y = c_y.clone();
                    }

                    if axis == "z" {
                        n_z = size_z-1-c_z;
                    } else {
                        n_z = c_z.clone();
                    }

                    let n_i = (n_y * size_x * size_z) + (n_z * size_x) + n_x;
                    newcpdata.set_entry(n_i as usize, clipboard.data.get_entry(i as usize));

                    // Ok now lets increment the coordinates for the next block
                    c_x = c_x + 1;

                    if c_x == size_x{
                        c_x = 0;
                        c_z = c_z + 1;

                        if c_z == size_z{
                            c_z = 0;
                            c_y = c_y + 1;
                        }
                    }
                }

                let mut cb = WorldEditClipboard {
                    offset_x: clipboard.offset_x,
                    offset_y: clipboard.offset_y,
                    offset_z: clipboard.offset_z,
                    size_x,
                    size_y,
                    size_z,
                    data: newcpdata,
                    // TODO: Get the block entities in the selection
                    block_entities: HashMap::new(), 
                };
                
                self.players[player].worldedit_clipboard = Some(cb);
                
                self.players[player].send_worldedit_message(&format!(
                    "Your selection was flipped oh the {:?} axis. ({:?})",
                    axis,
                    start_time.elapsed()
                ));
            } else {
                self.players[player].send_system_message("Your clipboard is empty!");
            }
        } else {
            self.players[player].send_system_message("//flip only works on the axises 'x', 'y', and 'z'");
        }
    }

    pub(super) fn worldedit_paste(&mut self, player: usize) {
        let start_time = Instant::now();

        if self.players[player].worldedit_clipboard.is_some() {
            // Here I am cloning the clipboard. This is bad. Don't do this.
            let cb = &self.players[player].worldedit_clipboard.clone().unwrap();
            let pos = BlockPos::new(
                self.players[player].x.floor() as i32,
                self.players[player].y.floor() as u32,
                self.players[player].z.floor() as i32,
            );
            self.paste_clipboard(cb, pos);
            self.players[player].send_worldedit_message(&format!(
                "Your clipboard was pasted. ({:?})",
                start_time.elapsed()
            ));
        } else {
            self.players[player].send_system_message("Your clipboard is empty!");
        }
    }

    pub(super) fn worldedit_load(&mut self, player: usize, file_name: &str) {
        let start_time = Instant::now();

        let clipboard = WorldEditClipboard::load_from_schematic(file_name);
        match clipboard {
            Some(cb) => {
                self.players[player].worldedit_clipboard = Some(cb);
                self.players[player].send_worldedit_message(&format!(
                    "The schematic was loaded to your clipboard. Do //paste to birth it into the world. ({:?})",
                    start_time.elapsed()
                ));
            }
            None => {
                self.players[player]
                    .send_error_message("There was an error loading the schematic.");
            }
        }
    }

    pub(super) fn worldedit_find(&mut self, player: usize, block_id: u32) {
        let start_time = Instant::now();

        if let Some(operation) = self.worldedit_start_operation(player) {
            for x in operation.x_range() {
                for y in operation.y_range() {
                    for z in operation.z_range() {
                        let block_pos = BlockPos::new(x, y as u32, z);
                        if self.get_block_raw(block_pos) == block_id {
                            self.players[player].send_worldedit_message(&format!(
                                "The block was found at {:?}",
                                block_pos
                            ));
                        }
                    }
                }
            }
            self.players[player]
                .send_worldedit_message(&format!("Done. ({:?})", start_time.elapsed()));
        }
    }
}
