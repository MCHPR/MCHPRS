mod commands;
pub mod database;
mod packets;
mod storage;
pub mod worldedit;

use crate::blocks::{Block, BlockEntity, BlockPos};
use crate::network::packets::clientbound::*;
use crate::network::packets::SlotData;
use crate::player::Player;
use crate::server::{BroadcastMessage, Message, PrivMessage};
use bus::BusReader;
use log::{debug, warn};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::{Duration, SystemTime};
use storage::{Chunk, ChunkData, PlotData};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum TickPriority {
    Highest,
    Higher,
    High,
    Normal,
}

impl TickPriority {
    fn values() -> [TickPriority; 4] {
        use TickPriority::*;
        [Highest, Higher, High, Normal]
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TickEntry {
    ticks_left: u32,
    tick_priority: TickPriority,
    pos: BlockPos,
}

pub struct Plot {
    message_receiver: BusReader<BroadcastMessage>,
    message_sender: Sender<Message>,
    priv_message_receiver: Receiver<PrivMessage>,
    // It's kinda dumb making this pub but it would be too much work to do it differently.
    pub players: Vec<Player>,
    tps: u32,
    to_be_ticked: Vec<TickEntry>,
    last_update_time: SystemTime,
    lag_time: Duration,
    last_player_time: SystemTime,
    sleep_time: Duration,
    running: bool,
    x: i32,
    z: i32,
    show_redstone: bool,
    always_running: bool,
    chunks: Vec<Chunk>,
    block_entities: HashMap<BlockPos, BlockEntity>,
}

impl Plot {
    fn get_chunk_index_for_chunk(&self, chunk_x: i32, chunk_z: i32) -> usize {
        let local_x = chunk_x - self.x * 16;
        let local_z = chunk_z - self.z * 16;
        (local_x * 16 + local_z).abs() as usize
    }

    fn get_chunk_index_for_block(&self, block_x: i32, block_z: i32) -> usize {
        let chunk_x = (block_x - (self.x << 8)) >> 4;
        let chunk_z = (block_z - (self.z << 8)) >> 4;
        ((chunk_x << 4) + chunk_z).abs() as usize
    }

    /// Sets a block in storage without sending a block change packet to the client. Returns true if a block was changed.
    fn set_block_raw(&mut self, pos: BlockPos, block: u32) -> bool {
        let chunk_index = self.get_chunk_index_for_block(pos.x, pos.z);
        if chunk_index >= 256 {
            return false;
        }
        let chunk = &mut self.chunks[chunk_index];
        chunk.set_block((pos.x & 0xF) as u32, pos.y, (pos.z & 0xF) as u32, block)
    }

    /// Returns true if a block was changed
    pub fn set_block(&mut self, pos: BlockPos, block: Block) -> bool {
        let block_id = Block::get_id(block);
        let changed = self.set_block_raw(pos, block_id);
        if changed {
            self.send_block_change(pos, block_id);
        }
        changed
    }

    fn get_block_raw(&self, pos: BlockPos) -> u32 {
        let chunk_index = self.get_chunk_index_for_block(pos.x, pos.z);
        if chunk_index >= 256 {
            return 0;
        }
        let chunk = &self.chunks[chunk_index];
        chunk.get_block((pos.x & 0xF) as u32, pos.y, (pos.z & 0xF) as u32)
    }

    pub fn get_block(&self, pos: BlockPos) -> Block {
        Block::from_block_state(self.get_block_raw(pos))
    }

    pub fn send_block_change(&mut self, pos: BlockPos, id: u32) {
        let block_change = C0CBlockChange {
            block_id: id as i32,
            x: pos.x,
            y: pos.y as i32,
            z: pos.z,
        }
        .encode();
        for player in &mut self.players {
            player.client.send_packet(&block_change);
        }
    }

    pub fn delete_block_entity(&mut self, pos: BlockPos) {
        self.block_entities.remove(&pos);
    }

    pub fn get_block_entity(&self, pos: BlockPos) -> Option<&BlockEntity> {
        self.block_entities.get(&pos)
    }

    pub fn set_block_entity(&mut self, pos: BlockPos, block_entity: BlockEntity) {
        self.block_entities.insert(pos, block_entity);
    }

    pub fn broadcast_chat_message(&mut self, message: String) {
        let broadcast_message = Message::ChatInfo(format!("Plot {}-{}", self.x, self.z), message);
        self.message_sender.send(broadcast_message).unwrap();
    }

    pub fn broadcast_plot_chat_message(&mut self, message: String) {
        for player in &mut self.players {
            player.send_chat_message(message.clone());
        }
    }

    pub fn schedule_tick(&mut self, pos: BlockPos, delay: u32, priority: TickPriority) {
        self.to_be_ticked.push(TickEntry {
            pos,
            ticks_left: delay,
            tick_priority: priority,
        });
        self.to_be_ticked
            .sort_by_key(|e| (e.ticks_left, e.tick_priority.clone()));
    }

    pub fn pending_tick_at(&mut self, pos: BlockPos) -> bool {
        self.to_be_ticked.iter().any(|e| e.pos == pos)
    }

    fn tick(&mut self) {
        for pending in &mut self.to_be_ticked {
            pending.ticks_left = pending.ticks_left.saturating_sub(1);
        }
        while self.to_be_ticked.first().map(|e| e.ticks_left).unwrap_or(1) == 0 {
            let entry = self.to_be_ticked.remove(0);
            self.get_block(entry.pos).tick(self, entry.pos);
        }
    }

    fn enter_plot(&mut self, mut player: Player) {
        debug!("Player enter plot!");
        self.save();
        for chunk in &self.chunks {
            player.client.send_packet(&chunk.encode_packet(true));
        }
        let spawn_player = C05SpawnPlayer {
            entity_id: player.entity_id as i32,
            uuid: player.uuid,
            on_ground: player.on_ground,
            pitch: player.pitch,
            yaw: player.yaw,
            x: player.x,
            y: player.y,
            z: player.z,
        }
        .encode();
        let mut metadata_entries = Vec::new();
        metadata_entries.push(C44EntityMetadataEntry {
            index: 16,
            metadata_type: 0,
            value: vec![player.skin_parts.bits() as u8],
        });
        let metadata = C44EntityMetadata {
            entity_id: player.entity_id as i32,
            metadata: metadata_entries,
        }
        .encode();
        for other_player in &mut self.players {
            other_player.client.send_packet(&spawn_player);
            other_player.client.send_packet(&metadata);

            let spawn_other_player = C05SpawnPlayer {
                entity_id: other_player.entity_id as i32,
                uuid: other_player.uuid,
                on_ground: other_player.on_ground,
                pitch: other_player.pitch,
                yaw: other_player.yaw,
                x: other_player.x,
                y: other_player.y,
                z: other_player.z,
            }
            .encode();
            player.client.send_packet(&spawn_other_player);

            if let Some(item) = &other_player.inventory[other_player.selected_slot as usize + 36] {
                let other_entity_equipment = C47EntityEquipment {
                    entity_id: other_player.entity_id as i32,
                    slot: 0, // Main hand
                    item: Some(SlotData {
                        item_count: item.count as i8,
                        item_id: item.item_type.get_id() as i32,
                        nbt: item.nbt.clone(),
                    }),
                }
                .encode();
                player.client.send_packet(&other_entity_equipment);
            }

            let mut other_metadata_entries = Vec::new();
            other_metadata_entries.push(C44EntityMetadataEntry {
                index: 16,
                metadata_type: 0,
                value: vec![other_player.skin_parts.bits() as u8],
            });
            let other_metadata = C44EntityMetadata {
                entity_id: other_player.entity_id as i32,
                metadata: other_metadata_entries,
            }
            .encode();
            player.client.send_packet(&other_metadata);
        }

        if let Some(item) = &player.inventory[player.selected_slot as usize + 36] {
            let entity_equipment = C47EntityEquipment {
                entity_id: player.entity_id as i32,
                slot: 0, // Main hand
                item: Some(SlotData {
                    item_count: item.count as i8,
                    item_id: item.item_type.get_id() as i32,
                    nbt: item.nbt.clone(),
                }),
            }
            .encode();
            for other_player in &mut self.players {
                other_player.client.send_packet(&entity_equipment);
            }
        }

        player.send_system_message(&format!("Entering plot ({}, {})", self.x, self.z));
        self.players.push(player);
    }

    fn destroy_entity(&mut self, entity_id: u32) {
        let destroy_entities = C38DestroyEntities {
            entity_ids: vec![entity_id as i32],
        }
        .encode();
        for player in &mut self.players {
            player.client.send_packet(&destroy_entities);
        }
    }

    fn leave_plot(&mut self, player_index: usize) -> Player {
        let mut player = self.players.remove(player_index);
        let mut entity_ids = Vec::new();
        for player in &self.players {
            entity_ids.push(player.entity_id as i32);
        }
        let destroy_other_entities = C38DestroyEntities { entity_ids }.encode();
        player.client.send_packet(&destroy_other_entities);
        let chunk_offset_x = self.x << 4;
        let chunk_offset_z = self.z << 4;
        for chunk in &self.chunks {
            player.client.send_packet(
                &C1EUnloadChunk {
                    chunk_x: chunk_offset_x + chunk.x,
                    chunk_z: chunk_offset_z + chunk.z,
                }
                .encode(),
            );
        }
        self.destroy_entity(player.entity_id);
        player
    }

    fn in_plot_bounds(plot_x: i32, plot_z: i32, x: i32, z: i32) -> bool {
        x >= plot_x * 256 && x < (plot_x + 1) * 256 && z >= plot_z * 256 && z < (plot_z + 1) * 256
    }

    fn update(&mut self) {
        // Handle messages from the message channel
        while let Ok(message) = self.message_receiver.try_recv() {
            match message {
                BroadcastMessage::Chat(message) => {
                    for player in &mut self.players {
                        player.send_raw_chat(message.clone());
                    }
                }
                BroadcastMessage::PlayerJoinedInfo(player_join_info) => {
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
                BroadcastMessage::PlayerLeft(uuid) => {
                    let player_info = C34PlayerInfo::RemovePlayer(vec![uuid]).encode();
                    for player in &mut self.players {
                        player.client.send_packet(&player_info);
                    }
                }
                BroadcastMessage::Shutdown => {
                    let mut players: Vec<Player> = self.players.drain(..).collect();
                    for player in players.iter_mut() {
                        player.save();
                        player.kick(
                            json!({
                                "text": "Server closed"
                            })
                            .to_string(),
                        );
                    }
                    self.always_running = false;
                    self.running = false;
                    return;
                }
            }
        }
        // Handle messages from the private message channel
        while let Ok(message) = self.priv_message_receiver.try_recv() {
            match message {
                PrivMessage::PlayerEnterPlot(player) => {
                    self.enter_plot(player);
                }
                PrivMessage::PlayerTeleportOther(mut player, username) => {
                    if let Some(other) = self.players.iter().find(|p| p.username == username) {
                        player.teleport(other.x, other.y, other.z);
                    }
                    self.enter_plot(player);
                }
            }
        }
        // Only tick if there are players in the plot
        if !self.players.is_empty() {
            self.last_player_time = SystemTime::now();
            if self.tps != 0 {
                let dur_per_tick = Duration::from_micros(1_000_000 / self.tps as u64);
                let elapsed_time = self.last_update_time.elapsed().unwrap();
                self.lag_time += elapsed_time;
                self.last_update_time = SystemTime::now();
                let ticks = self
                    .lag_time
                    .as_micros()
                    .checked_div(dur_per_tick.as_micros())
                    .unwrap_or_default();
                if ticks > 4000 {
                    warn!("Is the plot overloaded? Skipping {} ticks.", ticks);
                    self.lag_time = Duration::from_secs(0);
                }

                while self.lag_time >= dur_per_tick {
                    self.tick();
                    self.lag_time -= dur_per_tick;
                }
            }
        } else {
            // Unload plot after 600 seconds unless the plot should be always loaded
            if self.last_player_time.elapsed().unwrap().as_secs() > 600 && !self.always_running {
                self.running = false;
            }
        }
        // Update players
        for player in &mut self.players {
            player.update();
        }
        // Handle received packets
        for player in 0..self.players.len() {
            self.handle_packets_for_player(player);
        }

        let message_sender = &mut self.message_sender;

        // Remove disconnected players
        let mut disconnected_players = Vec::new();
        self.players.retain(|player| {
            let alive = player.client.alive;
            if !alive {
                player.save();
                message_sender
                    .send(Message::PlayerLeft(player.uuid))
                    .unwrap();
                disconnected_players.push(player.entity_id);
            }
            alive
        });
        for entity_id in disconnected_players {
            self.destroy_entity(entity_id);
        }

        // Remove players outside of the plot
        let mut outside_players = Vec::new();
        for player in 0..self.players.len() {
            if !Plot::in_plot_bounds(
                self.x,
                self.z,
                self.players[player].x as i32,
                self.players[player].z as i32,
            ) {
                outside_players.push(player);
            }
        }
        for player_index in outside_players {
            let player = self.leave_plot(player_index);
            let player_leave_plot = Message::PlayerLeavePlot(player);
            self.message_sender.send(player_leave_plot).unwrap();
        }
    }

    fn load_from_file(
        data: Vec<u8>,
        x: i32,
        z: i32,
        rx: BusReader<BroadcastMessage>,
        tx: Sender<Message>,
        priv_rx: Receiver<PrivMessage>,
        always_running: bool,
    ) -> Plot {
        let chunk_x_offset = x << 4;
        let chunk_z_offset = z << 4;
        let plot_data: PlotData = bincode::deserialize(&data).unwrap();
        let chunks: Vec<Chunk> = plot_data
            .chunk_data
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                Chunk::load(
                    chunk_x_offset + i as i32 / 16,
                    chunk_z_offset + i as i32 % 16,
                    c,
                )
            })
            .collect();
        Plot {
            last_player_time: SystemTime::now(),
            last_update_time: SystemTime::now(),
            lag_time: Duration::new(0, 0),
            sleep_time: Duration::from_micros((1_000_000 as u64).checked_div(plot_data.tps as u64).unwrap_or(0)),
            message_receiver: rx,
            message_sender: tx,
            priv_message_receiver: priv_rx,
            players: Vec::new(),
            running: true,
            show_redstone: plot_data.show_redstone,
            tps: plot_data.tps,
            x,
            z,
            always_running,
            chunks,
            to_be_ticked: plot_data.pending_ticks,
            block_entities: plot_data.block_entities,
        }
    }

    fn load(
        x: i32,
        z: i32,
        rx: BusReader<BroadcastMessage>,
        tx: Sender<Message>,
        priv_rx: Receiver<PrivMessage>,
        always_running: bool,
    ) -> Plot {
        if let Ok(data) = fs::read(format!("./world/plots/p{},{}", x, z)) {
            Plot::load_from_file(data, x, z, rx, tx, priv_rx, always_running)
        } else if Path::new("./world/plots/pTEMPLATE").exists() {
            let data = fs::read("./world/plots/pTEMPLATE").unwrap();
            Plot::load_from_file(data, x, z, rx, tx, priv_rx, always_running)
        } else {
            debug!(
                "Plot {},{} does not exist and no template was found, generating now.",
                x, z
            );
            let chunk_x_offset = x << 4;
            let chunk_z_offset = z << 4;
            let mut chunks = Vec::new();
            for chunk_x in 0..16 {
                for chunk_z in 0..16 {
                    chunks.push(Chunk::generate(
                        8,
                        chunk_x + chunk_x_offset,
                        chunk_z + chunk_z_offset,
                    ));
                }
            }
            Plot {
                last_player_time: SystemTime::now(),
                last_update_time: SystemTime::now(),
                lag_time: Duration::new(0, 0),
                sleep_time: Duration::from_millis(30),
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
                to_be_ticked: Vec::new(),
                block_entities: HashMap::new(),
            }
        }
    }

    fn save(&self) {
        debug!("Saving plot {},{}", self.x, self.z);
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(format!("./world/plots/p{},{}", self.x, self.z))
            .unwrap();
        let chunk_data: Vec<ChunkData> = self.chunks.iter().map(|c| c.save()).collect();
        let encoded: Vec<u8> = bincode::serialize(&PlotData {
            tps: self.tps,
            show_redstone: self.show_redstone,
            chunk_data,
            pending_ticks: self.to_be_ticked.clone(),
            block_entities: self.block_entities.clone(),
        })
        .unwrap();
        file.write_all(&encoded).unwrap();
        file.sync_data().unwrap();
    }

    fn run(&mut self, initial_player: Option<Player>) {
        debug!("Running new plot!");
        if let Some(player) = initial_player {
            debug!("Sending initial player into plot!");
            self.enter_plot(player);
        }
        while self.running {
            self.update();
            thread::sleep(self.sleep_time);
        }
    }

    pub fn load_and_run(
        x: i32,
        z: i32,
        rx: BusReader<BroadcastMessage>,
        tx: Sender<Message>,
        priv_rx: Receiver<PrivMessage>,
        always_running: bool,
        initial_player: Option<Player>,
    ) {
        let mut plot = Plot::load(x, z, rx, tx, priv_rx, always_running);
        thread::Builder::new()
            .name(format!("p{},{}", x, z))
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
            for player in &mut self.players {
                player.save();
                // Give the player the bad news.
                player.kick(
                    r#"{ "text": "The plot you were previously in has crashed!", "color": "red" }"#
                        .to_owned(),
                );
            }
        }
        self.save();
        debug!("Plot {},{} unloaded", self.x, self.z);
        self.message_sender
            .send(Message::PlotUnload(self.x, self.z))
            .unwrap();
    }
}

#[test]
fn chunk_save_and_load_test() {
    let mut chunk = Chunk::empty(1, 1);
    chunk.set_block(13, 63, 12, 332);
    chunk.set_block(13, 62, 12, 331);
    let chunk_data = chunk.save();
    let loaded_chunk = Chunk::load(1, 1, chunk_data);
    assert_eq!(loaded_chunk.get_block(13, 63, 12), 332);
    assert_eq!(loaded_chunk.get_block(13, 62, 12), 331);
    assert_eq!(loaded_chunk.get_block(13, 64, 12), 0);
}
