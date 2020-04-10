use crate::blocks::Block;
use crate::network::packets::clientbound::*;
use crate::network::packets::serverbound::*;
use crate::network::packets::{PacketDecoder, PacketEncoder};
use crate::player::{Player, SkinParts};
use crate::server::{Message, PrivMessage};
use bus::BusReader;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::mem;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

pub struct Plot {
    players: Vec<Player>,
    tps: u32,
    message_receiver: BusReader<Message>,
    message_sender: Sender<Message>,
    priv_message_receiver: Receiver<PrivMessage>,
    last_player_time: SystemTime,
    running: bool,
    x: i32,
    z: i32,
    show_redstone: bool,
    always_running: bool,
    chunks: Vec<Chunk>,
}

impl Plot {
    fn get_chunk_index(block_x: i32, block_z: i32) -> usize {
        let chunk_x = block_x >> 4;
        let chunk_z = block_z >> 4;
        (chunk_x * 8 + chunk_z).abs() as usize
    }

    /// Sets a block in storage without sending a block change packet to the client. Returns true if a block was changed.
    fn set_block_raw(&mut self, x: i32, y: u32, z: i32, block: u32) -> bool {
        let chunk = &mut self.chunks[Plot::get_chunk_index(x, z)];
        chunk.set_block((x & 0xF) as u32, y, (z & 0xF) as u32, block)
    }

    /// Returns true if a block was changed
    fn set_block(&mut self, x: i32, y: u32, z: i32, block: Block) -> bool {
        let block_id = Block::get_id(&block);
        let changed = self.set_block_raw(x, y, z, block_id);
        let block_change = C0CBlockChange {
            block_id: block_id as i32,
            x,
            y: y as i32,
            z,
        }
        .encode();
        for player in &mut self.players {
            player.client.send_packet(&block_change);
        }
        changed
    }

    fn get_block(&mut self, x: i32, y: u32, z: i32) -> Block {
        let chunk = &self.chunks[Plot::get_chunk_index(x, z)];
        Block::from_block_state(chunk.get_block((x & 0xF) as u32, y, (z & 0xF) as u32))
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
            player.send_system_message("First position is not set!".to_string());
            return None;
        }
        if let Some(pos) = player.second_position {
            second_pos = pos;
        } else {
            player.send_system_message("Second position is not set!".to_string());
            return None;
        }
        if !Plot::in_plot_bounds(self.x, self.z, first_pos.0, first_pos.2) {
            player.send_system_message("First position is outside plot bounds!".to_string());
        }
        if !Plot::in_plot_bounds(self.x, self.z, first_pos.0, first_pos.2) {
            player.send_system_message("Second position is outside plot bounds!".to_string());
        }
        Some((first_pos, second_pos))
    }

    fn worldedit_set(&mut self, player: usize, block: Block) {
        if let Some((first_pos, second_pos)) = self.worldedit_verify_positions(player) {
            let mut blocks_updated = 0;
            let x_start = std::cmp::min(first_pos.0, second_pos.0);
            let x_end = std::cmp::max(first_pos.0, second_pos.0);
            let y_start = std::cmp::min(first_pos.1, second_pos.1);
            let y_end = std::cmp::max(first_pos.1, second_pos.1);
            let z_start = std::cmp::min(first_pos.2, second_pos.2);
            let z_end = std::cmp::max(first_pos.2, second_pos.2);
            for x in x_start..=x_end {
                for y in y_start..=y_end {
                    for z in z_start..=z_end {
                        if self.set_block(x, y as u32, z, block) {
                            blocks_updated += 1;
                        }
                    }
                }
            }
            self.players[player].send_worldedit_message(format!(
                "Operation completed: {} block(s) updated",
                blocks_updated
            ));
        }
    }

    fn tick(&mut self) {}

    fn enter_plot(&mut self, mut player: Player) {
        self.save();
        for chunk in &self.chunks {
            player.client.send_packet(&chunk.encode_packet());
        }
        player.send_system_message(format!("Entering plot ({}, {})", self.x, self.z));
        self.players.push(player);
    }

    /// Blocks the thread until the arc has no other strong references,
    /// this will then return the player.
    fn receive_player(player_arc: Arc<Player>) -> Player {
        // Maybe we could store a list of players waiting to be received instead of
        // blocking the thread. Just maybe...
        while Arc::strong_count(&player_arc) > 1 {
            thread::sleep(Duration::from_millis(10))
        }
        Arc::try_unwrap(player_arc).unwrap()
    }

    fn in_plot_bounds(plot_x: i32, plot_z: i32, x: i32, z: i32) -> bool {
        x >= plot_x * 128 && x < (plot_x + 1) * 128 && z >= plot_z * 128 && z < (plot_z + 1) * 128
    }

    fn update(&mut self) {
        // Handle messages from the private message channel
        while let Ok(message) = self.priv_message_receiver.try_recv() {
            match message {
                PrivMessage::PlayerEnterPlot(player) => {
                    self.enter_plot(player);
                }
            }
        }
        // Handle messages from the message channel
        while let Ok(message) = self.message_receiver.try_recv() {
            match message {
                Message::PlayerTeleportOther(player, other_player) => {
                    for p in self.players.iter() {
                        if p.username == other_player {
                            let mut player = Plot::receive_player(player);
                            player.teleport(p.x, p.y, p.z);
                            self.enter_plot(player);
                            break;
                        }
                    }
                }
                Message::Chat(message) => {
                    for player in &mut self.players {
                        player.send_raw_chat(message.clone());
                    }
                }
                Message::PlayerJoinedInfo(player_join_info) => {
                    let player_info = C34PlayerInfo::AddPlayer(vec![C34PlayerInfoAddPlayer {
                        name: player_join_info.username,
                        properties: Vec::new(),
                        gamemode: 1,
                        ping: 0,
                        uuid: player_join_info.uuid,
                        display_name: None,
                    }])
                    .encode();
                    for player in &mut self.players {
                        player.client.send_packet(&player_info);
                    }
                }
                Message::PlayerLeft(uuid) => {
                    let player_info = C34PlayerInfo::RemovePlayer(vec![uuid]).encode();
                    for player in &mut self.players {
                        player.client.send_packet(&player_info);
                    }
                }
                _ => {}
            }
        }
        // Only tick if there are players in the plot
        if !self.players.is_empty() {
            self.last_player_time = SystemTime::now();
            self.tick();
        } else {
            // Unload plot after 600 seconds unless the plot should be always loaded
            if self.last_player_time.elapsed().unwrap().as_secs() > 600 && !self.always_running {
                self.running = false;
            }
        }
        let message_sender = &mut self.message_sender;
        // Update players
        for player in &mut self.players {
            player.update();
        }
        // Check if connection to player is still active
        self.players.retain(|player| {
            let alive = player.client.alive;
            if !alive {
                player.save();
                message_sender
                    .send(Message::PlayerLeft(player.uuid))
                    .unwrap();
            }
            alive
        });
        // Handle received packets
        for player in 0..self.players.len() {
            let packets: Vec<PacketDecoder> =
                self.players[player].client.packets.drain(..).collect();
            for packet in packets {
                match packet.packet_id {
                    0x03 => {
                        //let player = &mut self.players[player];
                        let chat_message = S03ChatMessage::decode(packet).unwrap();
                        let message = chat_message.message;
                        if message.starts_with('/') {
                            let mut args: Vec<&str> = message.split(' ').collect();
                            match args.remove(0) {
                                "//1" | "//pos1" => {
                                    let player = &mut self.players[player];
                                    player.set_first_position(
                                        player.x as i32,
                                        player.y as i32,
                                        player.z as i32,
                                    );
                                }
                                "//2" | "//pos2" => {
                                    let player = &mut self.players[player];
                                    player.set_second_position(
                                        player.x as i32,
                                        player.y as i32,
                                        player.z as i32,
                                    );
                                }
                                "//set" => {
                                    let block = Block::from_name(&args[0]);
                                    if let Some(block) = block {
                                        self.worldedit_set(player, block);
                                    } else {
                                        self.players[player].send_system_message("Invalid block. Note that not all blocks are supported.".to_string());
                                    }
                                }
                                "/setblock" => {
                                    // TODO: Remove or make better
                                    let player = &self.players[player];
                                    let block = Block::from_name(&args[0]).unwrap();
                                    self.set_block(
                                        player.x as i32,
                                        player.y as u32,
                                        player.z as i32,
                                        block,
                                    );
                                }
                                "/tp" => {
                                    if args.len() == 3 {
                                        let x;
                                        let y;
                                        let z;
                                        if let Ok(x_arg) = args[0].parse::<f64>() {
                                            x = x_arg;
                                        } else {
                                            self.players[player].send_system_message(
                                                "Unable to parse x coordinate!".to_string(),
                                            );
                                            return;
                                        }
                                        if let Ok(y_arg) = args[1].parse::<f64>() {
                                            y = y_arg;
                                        } else {
                                            self.players[player].send_system_message(
                                                "Unable to parse y coordinate!".to_string(),
                                            );
                                            return;
                                        }
                                        if let Ok(z_arg) = args[2].parse::<f64>() {
                                            z = z_arg;
                                        } else {
                                            self.players[player].send_system_message(
                                                "Unable to parse z coordinate!".to_string(),
                                            );
                                            return;
                                        }
                                        self.players[player].teleport(x, y, z);
                                    } else if args.len() == 1 {
                                        self.players[player].send_system_message(
                                            "TODO: teleport to player".to_string(),
                                        );
                                    } else {
                                        self.players[player].send_system_message(
                                            "Wrong number of arguments for teleport command!"
                                                .to_string(),
                                        );
                                    }
                                }
                                _ => self.players[player]
                                    .send_system_message("Command not found!".to_string()),
                            }
                        } else {
                            let player = &self.players[player];
                            let broadcast_message = Message::Chat(
                                json!({ "text": format!("{}: {}", player.username, message) })
                                    .to_string(),
                            );
                            self.message_sender.send(broadcast_message).unwrap();
                        }
                    }
                    0x05 => {
                        let player = &mut self.players[player];
                        let client_settings = S05ClientSettings::decode(packet).unwrap();
                        player.skin_parts = SkinParts::from_bits_truncate(
                            client_settings.displayed_skin_parts as u32,
                        );
                        let metadata_entry = C44EntityMetadataEntry {
                            index: 16,
                            metadata_type: 0,
                            value: vec![player.skin_parts.bits() as u8],
                        };
                        let entity_metadata = C44EntityMetadata {
                            entity_id: player.entity_id as i32,
                            metadata: vec![metadata_entry],
                        }
                        .encode();
                        for player in &mut self.players {
                            player.client.send_packet(&entity_metadata);
                        }
                    }
                    0x0F => self.players[player].last_keep_alive_received = Instant::now(),
                    0x11 => {
                        let player = &mut self.players[player];
                        let player_position = S11PlayerPosition::decode(packet).unwrap();
                        player.x = player_position.x;
                        player.y = player_position.y;
                        player.z = player_position.z;
                        player.on_ground = player_position.on_ground;
                    }
                    0x12 => {
                        let player = &mut self.players[player];
                        let player_position_and_rotation =
                            S12PlayerPositionAndRotation::decode(packet).unwrap();
                        player.x = player_position_and_rotation.x;
                        player.y = player_position_and_rotation.y;
                        player.z = player_position_and_rotation.z;
                        player.yaw = player_position_and_rotation.yaw;
                        player.pitch = player_position_and_rotation.pitch;
                        player.on_ground = player_position_and_rotation.on_ground;
                    }
                    0x13 => {
                        let player = &mut self.players[player];
                        let player_rotation = S13PlayerRotation::decode(packet).unwrap();
                        player.yaw = player_rotation.yaw;
                        player.pitch = player_rotation.pitch;
                        player.on_ground = player_rotation.on_ground;
                    }
                    0x14 => {
                        let player = &mut self.players[player];
                        let player_movement = S14PlayerMovement::decode(packet).unwrap();
                        player.on_ground = player_movement.on_ground;
                    }
                    _ => {}
                }
            }
        }
        // Check if a player has left the plot
        for player in 0..self.players.len() {
            if !Plot::in_plot_bounds(
                self.x,
                self.z,
                self.players[player].x as i32,
                self.players[player].z as i32,
            ) {
                let player_leave_plot =
                    Message::PlayerLeavePlot(Arc::from(self.players.remove(player)));
                self.message_sender.send(player_leave_plot).unwrap();
            }
        }
    }

    fn load(
        x: i32,
        z: i32,
        rx: BusReader<Message>,
        tx: Sender<Message>,
        priv_rx: Receiver<PrivMessage>,
        always_running: bool,
    ) -> Plot {
        let chunk_x_offset = x << 3;
        let chunk_z_offset = z << 3;
        if let Ok(data) = fs::read(format!("./world/plots/p{}:{}", x, z)) {
            // Load plot from file
            // TODO: Handle format error
            let plot_data: PlotData = bincode::deserialize(&data).unwrap();
            println!("{:?}", plot_data);
            let chunks: Vec<Chunk> = plot_data
                .chunk_data
                .into_iter()
                .enumerate()
                .map(|(i, c)| {
                    Chunk::load(
                        chunk_x_offset + i as i32 / 8,
                        chunk_z_offset + i as i32 % 8,
                        c,
                    )
                })
                .collect();
            Plot {
                last_player_time: SystemTime::now(),
                message_receiver: rx,
                message_sender: tx,
                priv_message_receiver: priv_rx,
                players: Vec::new(),
                running: true,
                show_redstone: plot_data.show_redstone,
                tps: plot_data.tps as u32,
                x,
                z,
                always_running,
                chunks,
            }
        } else {
            // Create a new plot with empty chunks
            let mut chunks = Vec::new();
            for chunk_x in 0..8 {
                for chunk_z in 0..8 {
                    chunks.push(Chunk::empty(
                        chunk_x + chunk_x_offset,
                        chunk_z + chunk_z_offset,
                    ));
                }
            }
            Plot {
                last_player_time: SystemTime::now(),
                message_receiver: rx,
                message_sender: tx,
                priv_message_receiver: priv_rx,
                players: Vec::new(),
                running: true,
                show_redstone: true,
                tps: 20,
                x,
                z,
                always_running,
                chunks,
            }
        }
    }

    fn save(&self) {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(format!("./world/plots/p{}:{}", self.x, self.z))
            .unwrap();
        let chunk_data: Vec<ChunkData> = self.chunks.iter().map(|c| c.save()).collect();
        let encoded: Vec<u8> = bincode::serialize(&PlotData {
            tps: self.tps as i32,
            show_redstone: self.show_redstone,
            chunk_data,
        })
        .unwrap();
        file.write_all(&encoded).unwrap();
        file.sync_data().unwrap();
    }

    fn run(&mut self, initial_player: Option<Player>) {
        println!("Running new plot!");
        if let Some(player) = initial_player {
            self.enter_plot(player);
        }
        while self.running {
            self.update();
            thread::sleep(Duration::from_millis(100));
        }
    }

    pub fn load_and_run(
        x: i32,
        z: i32,
        rx: BusReader<Message>,
        tx: Sender<Message>,
        priv_rx: Receiver<PrivMessage>,
        always_running: bool,
        initial_player: Option<Player>,
    ) {
        let mut plot = Plot::load(x, z, rx, tx, priv_rx, always_running);
        thread::Builder::new()
            .name(format!("p{}:{}", x, z))
            .spawn(move || {
                plot.run(initial_player);
            })
            .unwrap();
    }
}

impl Drop for Plot {
    fn drop(&mut self) {
        if !self.players.is_empty() {
            // TODO: send all players to spawn and send them message along the lines of:
            // "The plot you were previously in has crashed, you have been teleported to the spawn plot."
            for _player in &self.players {}
        }
        self.save();
        self.message_sender
            .send(Message::PlotUnload(self.x, self.z))
            .unwrap();
    }
}

struct BitBuffer {
    bits_per_entry: u8,
    entries: usize,
    longs: Vec<u64>,
}

impl BitBuffer {
    fn create(bits_per_entry: u8, entries: usize) -> BitBuffer {
        let longs_len = entries * bits_per_entry as usize / 64;
        let longs = vec![0; longs_len];
        BitBuffer {
            bits_per_entry,
            longs,
            entries,
        }
    }

    fn load(bits_per_entry: u8, longs: Vec<u64>) -> BitBuffer {
        let entries = longs.len() * 64 / bits_per_entry as usize;
        BitBuffer {
            bits_per_entry,
            longs,
            entries,
        }
    }

    fn get_entry(&self, index: usize) -> u32 {
        let long_index = (self.bits_per_entry as usize * index) >> 6;
        let index_in_long = (self.bits_per_entry as usize * index) & 0x3F;
        let bitmask = ((1u128 << self.bits_per_entry) - 1) << index_in_long;

        let mut long_long = self.longs[long_index] as u128;
        if self.longs.len() > long_index + 1 {
            long_long |= (self.longs[long_index + 1] as u128) << 64;
        }
        // if ((bitmask & long_long) >> index_in_long) != 0 {
        //     println!("long:    {:0128b}\nbitmask: {:128b} {}", long_long, bitmask, self.bits_per_entry);
        // }
        ((bitmask & long_long) >> index_in_long) as u32
    }

    fn set_entry(&mut self, index: usize, val: u32) {
        let long_index = (self.bits_per_entry as usize * index) >> 6;
        let index_in_long = (self.bits_per_entry as usize * index) & 0x3F;
        let bitmask = ((1u128 << self.bits_per_entry) - 1) << index_in_long;

        self.longs[long_index] = (self.longs[long_index] & !(bitmask as u64)) // Remove old value
            | ((val as u128) << index_in_long as u128) as u64; // Insert new value, TODO: use a better way than `as u128`

        // Check if the value overlaps into the next long
        if index_in_long + self.bits_per_entry as usize > 64 {
            self.longs[long_index + 1] = (self.longs[long_index + 1] & !(bitmask >> 64) as u64) // Remove old value
                | (val >> (64 - index_in_long)) as u64; // Insert new value
        }
    }
}

struct PalettedBitBuffer {
    data: BitBuffer,
    palatte: Vec<u32>,
    max_entries: u32,
    use_palatte: bool,
}

impl PalettedBitBuffer {
    fn new() -> PalettedBitBuffer {
        let mut palatte = Vec::new();
        palatte.push(0);
        PalettedBitBuffer {
            data: BitBuffer::create(4, 4096),
            palatte,
            max_entries: 16,
            use_palatte: true,
        }
    }

    fn load(bits_per_entry: u8, longs: Vec<u64>, palatte: Vec<u32>) -> PalettedBitBuffer {
        PalettedBitBuffer {
            data: BitBuffer::load(bits_per_entry, longs),
            palatte,
            use_palatte: bits_per_entry < 9,
            max_entries: 1 << bits_per_entry,
        }
    }

    fn resize_buffer(&mut self) {
        let old_bits_per_entry = self.data.bits_per_entry;
        if old_bits_per_entry + 1 > 8 {
            let mut old_buffer = BitBuffer::create(14, 4096);
            mem::swap(&mut self.data, &mut old_buffer);
            self.max_entries = 1 << 14;
            for entry in 0..old_buffer.entries {
                self.data
                    .set_entry(entry, self.palatte[old_buffer.get_entry(entry) as usize]);
            }
        } else {
            let mut old_buffer = BitBuffer::create(old_bits_per_entry + 1, 4096);
            mem::swap(&mut self.data, &mut old_buffer);
            self.max_entries <<= 1;
            for entry in 0..old_buffer.entries {
                self.data.set_entry(entry, old_buffer.get_entry(entry));
            }
        };
    }

    fn get_entry(&self, index: usize) -> u32 {
        if self.use_palatte {
            self.palatte[self.data.get_entry(index) as usize]
        } else {
            self.data.get_entry(index)
        }
    }

    fn set_entry(&mut self, index: usize, val: u32) {
        if self.use_palatte {
            if let Some(palatte_index) = self.palatte.iter().position(|x| x == &val) {
                self.data.set_entry(index, palatte_index as u32);
            } else {
                if self.palatte.len() + 1 > self.max_entries as usize {
                    self.resize_buffer();
                }
                let palatte_index = self.palatte.len();
                self.palatte.push(val);
                self.data.set_entry(index, palatte_index as u32);
            }
        } else {
            self.data.set_entry(index, val);
        }
    }
}

struct ChunkSection {
    y: u8,
    buffer: PalettedBitBuffer,
    block_count: u32,
}

impl ChunkSection {
    fn get_index(x: u32, y: u32, z: u32) -> usize {
        ((y << 8) | (z << 4) | x) as usize
    }

    fn get_block(&self, x: u32, y: u32, z: u32) -> u32 {
        self.buffer.get_entry(ChunkSection::get_index(x, y, z))
    }

    /// Sets a block in the chunk sections. Returns true if a block was changed.
    fn set_block(&mut self, x: u32, y: u32, z: u32, block: u32) -> bool {
        let old_block = self.get_block(x, y, z);
        if old_block == 0 && block != 0 {
            self.block_count += 1;
        } else if old_block != 0 && block == 0 {
            self.block_count -= 1;
        }
        self.buffer
            .set_entry(ChunkSection::get_index(x, y, z), block);
        old_block != block
    }

    fn load(data: ChunkSectionData) -> ChunkSection {
        let loaded_longs = data.data.into_iter().map(|x| x as u64).collect();
        let bits_per_entry = data.bits_per_block as u8;
        let palette = data.palatte.into_iter().map(|x| x as u32).collect();
        let buffer = PalettedBitBuffer::load(bits_per_entry, loaded_longs, palette);
        ChunkSection {
            y: data.y as u8,
            buffer,
            block_count: data.block_count as u32,
        }
    }

    fn save(&self) -> ChunkSectionData {
        let longs: Vec<i64> = self
            .buffer
            .data
            .longs
            .clone()
            .into_iter()
            .map(|x| x as i64)
            .collect();
        let palatte: Vec<i32> = self
            .buffer
            .palatte
            .clone()
            .into_iter()
            .map(|x| x as i32)
            .collect();
        ChunkSectionData {
            data: longs,
            palatte,
            bits_per_block: self.buffer.data.bits_per_entry as i8,
            y: self.y as i8,
            block_count: self.block_count as i32,
        }
    }

    fn new(y: u8) -> ChunkSection {
        ChunkSection {
            y,
            buffer: PalettedBitBuffer::new(),
            block_count: 10,
        }
    }

    fn encode_packet(&self) -> C22ChunkDataSection {
        C22ChunkDataSection {
            bits_per_block: self.buffer.data.bits_per_entry,
            block_count: self.block_count as i16,
            data_array: self.buffer.data.longs.clone(),
            palette: if self.buffer.use_palatte {
                Some(
                    self.buffer
                        .palatte
                        .clone()
                        .into_iter()
                        .map(|x| x as i32)
                        .collect(),
                )
            } else {
                None
            },
        }
    }
}

struct Chunk {
    sections: Vec<ChunkSection>,
    x: i32,
    z: i32,
}

impl Chunk {
    fn encode_packet(&self) -> PacketEncoder {
        let mut heightmap_buffer = BitBuffer::create(9, 256);
        for x in 0..16 {
            for z in 0..16 {
                heightmap_buffer
                    .set_entry((x * 16) + z, self.get_top_most_block(x as u32, z as u32));
            }
        }

        let mut chunk_sections = Vec::new();
        let mut bitmask = 0;
        for section in &self.sections {
            bitmask |= 1 << section.y;
            chunk_sections.push(section.encode_packet());
        }
        let mut heightmaps = nbt::Blob::new();
        let heightmap_longs: Vec<i64> = heightmap_buffer
            .longs
            .into_iter()
            .map(|x| x as i64)
            .collect();
        heightmaps
            .insert("MOTION_BLOCKING", heightmap_longs)
            .unwrap();
        C22ChunkData {
            biomes: Some(vec![0; 1024]),
            chunk_sections,
            chunk_x: self.x,
            chunk_z: self.z,
            full_chunk: true,
            heightmaps,
            primary_bit_mask: bitmask as i32,
        }
        .encode()
    }

    fn get_top_most_block(&self, x: u32, z: u32) -> u32 {
        let mut top_most = 0;
        for section in &self.sections {
            for y in (0..15).rev() {
                let block_state = section.get_block(x, y, z);
                if block_state != 0 && top_most < y + section.y as u32 * 16 {
                    top_most = section.y as u32 * 16;
                }
            }
        }
        top_most
    }

    /// Sets a block in the chunk. Returns true if a block was changed.
    fn set_block(&mut self, x: u32, y: u32, z: u32, block_id: u32) -> bool {
        let section_y = (y >> 4) as u8;
        if let Some(section) = self.sections.iter_mut().find(|s| s.y == section_y) {
            section.set_block(x, y & 0xF, z, block_id)
        } else if block_id != 0 {
            let mut section = ChunkSection::new(section_y);
            section.set_block(x, y & 0xF, z, block_id);
            self.sections.push(section);
            true
        } else {
            // The block was air so a new chunk section does not need to be created.
            false
        }
    }

    fn get_block(&self, x: u32, y: u32, z: u32) -> u32 {
        let section_y = (y / 16) as u8;
        if let Some(section) = self.sections.iter().find(|s| s.y == section_y) {
            section.get_block(x, y & 0xF, z)
        } else {
            0
        }
    }

    fn save(&self) -> ChunkData {
        ChunkData {
            sections: self.sections.iter().map(|s| s.save()).collect(),
        }
    }

    fn load(x: i32, z: i32, chunk_data: ChunkData) -> Chunk {
        println!("Loading chunk {},{}", x, z);
        Chunk {
            x,
            z,
            sections: chunk_data
                .sections
                .into_iter()
                .map(ChunkSection::load)
                .collect(),
        }
    }

    fn empty(x: i32, z: i32) -> Chunk {
        Chunk {
            sections: Vec::new(),
            x,
            z,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct ChunkSectionData {
    y: i8,
    data: Vec<i64>,
    palatte: Vec<i32>,
    bits_per_block: i8,
    block_count: i32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct ChunkData {
    sections: Vec<ChunkSectionData>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PlotData {
    tps: i32,
    show_redstone: bool,
    chunk_data: Vec<ChunkData>,
}

#[test]
fn chunk_save_test() {
    let mut chunk = Chunk::empty(1, 1);
    chunk.set_block(13, 63, 12, 332);
    chunk.set_block(13, 62, 12, 331);
    let chunk_data = chunk.save();
    let loaded_chunk = Chunk::load(1, 1, chunk_data);
    assert_eq!(loaded_chunk.get_block(13, 63, 12), 332);
    assert_eq!(loaded_chunk.get_block(13, 62, 12), 331);
    assert_eq!(loaded_chunk.get_block(13, 64, 12), 0);
}