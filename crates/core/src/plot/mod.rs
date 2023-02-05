pub mod commands;
mod data;
pub mod database;
mod monitor;
mod packet_handlers;
mod scoreboard;
pub mod worldedit;

use crate::blocks::Block;
use crate::chat::ChatComponent;
use crate::config::CONFIG;
use crate::player::{EntityId, Gamemode, PacketSender, Player, PlayerPos};
use crate::redpiler::{Compiler, CompilerOptions};
use crate::server::{BroadcastMessage, Message, PrivMessage};
use crate::utils::HyphenatedUUID;
use crate::world::storage::Chunk;
use crate::world::World;
use anyhow::Context;
use bus::BusReader;
use mchprs_blocks::block_entities::BlockEntity;
use mchprs_blocks::{BlockFace, BlockPos};
use mchprs_network::packets::clientbound::*;
use mchprs_network::packets::SlotData;
use mchprs_network::PlayerPacketSender;
use mchprs_save_data::plot_data::{ChunkData, PlotData, Tps};
use mchprs_world::{TickEntry, TickPriority};
use monitor::TimingsMonitor;
use scoreboard::RedpilerState;
use serde_json::json;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tracing::{debug, error, warn};

use self::data::sleep_time_for_tps;
use self::scoreboard::Scoreboard;

/// The width of a plot (2^n)
pub const PLOT_SCALE: u32 = 4;

/// The width of a plot counted in chunks
pub const PLOT_WIDTH: i32 = 2i32.pow(PLOT_SCALE);
/// The plot width in blocks
pub const PLOT_BLOCK_WIDTH: i32 = PLOT_WIDTH * 16;
pub const NUM_CHUNKS: usize = PLOT_WIDTH.pow(2) as usize;

pub const WORLD_SEND_RATE: Duration = Duration::from_millis(15);

pub struct Plot {
    pub world: PlotWorld,
    pub players: Vec<Player>,
    pub redpiler: Compiler,

    // Thread communication
    message_receiver: BusReader<BroadcastMessage>,
    message_sender: Sender<Message>,
    priv_message_receiver: Receiver<PrivMessage>,

    locked_players: HashSet<EntityId>,

    // Timings
    tps: Tps,
    last_update_time: Instant,
    lag_time: Duration,
    last_nspt: Option<Duration>,
    timings: TimingsMonitor,
    /// The last time a player was in this plot
    last_player_time: Instant,
    /// The last time the world changes were sent to the player
    last_world_send_time: Instant,
    /// The duration we should sleep for after every update
    sleep_time: Duration,
    /// When this is false, the update loop will end and the thread will stop.
    /// This will be set to false if no players are on the plot for a certain amount of time.
    running: bool,
    /// If true, the plot will remain running even if no players are on for a long time.
    always_running: bool,
    auto_redpiler: bool,

    owner: Option<u128>,
    async_rt: Runtime,
    scoreboard: Scoreboard,
}

pub struct PlotWorld {
    pub x: i32,
    pub z: i32,
    pub chunks: Vec<Chunk>,
    pub to_be_ticked: Vec<TickEntry>,
    pub packet_senders: Vec<PlayerPacketSender>,
}

impl PlotWorld {
    fn get_chunk_index_for_chunk(&self, chunk_x: i32, chunk_z: i32) -> usize {
        let local_x = chunk_x - self.x * PLOT_WIDTH;
        let local_z = chunk_z - self.z * PLOT_WIDTH;
        (local_x * PLOT_WIDTH + local_z).unsigned_abs() as usize
    }

    fn get_chunk_index_for_block(&self, block_x: i32, block_z: i32) -> Option<usize> {
        let chunk_x = (block_x - (self.x * PLOT_BLOCK_WIDTH)) >> 4;
        let chunk_z = (block_z - (self.z * PLOT_BLOCK_WIDTH)) >> 4;
        if chunk_x >= PLOT_WIDTH || chunk_z >= PLOT_WIDTH {
            return None;
        }
        Some(((chunk_x << PLOT_SCALE) + chunk_z).unsigned_abs() as usize)
    }

    fn flush_block_changes(&mut self) {
        for packet in self.chunks.iter_mut().flat_map(|c| c.multi_blocks()) {
            let encoded = packet.encode();
            for player in &self.packet_senders {
                player.send_packet(&encoded);
            }
        }
        for chunk in &mut self.chunks {
            chunk.reset_multi_blocks();
        }
    }

    pub fn get_corners(&self) -> (BlockPos, BlockPos) {
        const W: i32 = PLOT_BLOCK_WIDTH;
        let first_pos = BlockPos::new(self.x * W, 0, self.z * W);
        let second_pos = BlockPos::new((self.x + 1) * W - 1, 255, (self.z + 1) * W - 1);
        (first_pos, second_pos)
    }
}

impl World for PlotWorld {
    /// Sets a block in storage. Returns true if a block was changed.
    fn set_block_raw(&mut self, pos: BlockPos, block: u32) -> bool {
        let chunk_index = match self.get_chunk_index_for_block(pos.x, pos.z) {
            Some(idx) => idx,
            None => return false,
        };

        // Check to see if block is within height limit
        if pos.y >= 256 || pos.y < 0 {
            return false;
        }

        let chunk = &mut self.chunks[chunk_index];
        chunk.set_block(
            (pos.x & 0xF) as u32,
            pos.y as u32,
            (pos.z & 0xF) as u32,
            block,
        )
    }

    /// Sets the block at `pos`.
    fn set_block(&mut self, pos: BlockPos, block: Block) -> bool {
        let block_id = Block::get_id(block);
        self.set_block_raw(pos, block_id)
    }

    /// Returns the block state id of the block at `pos`
    fn get_block_raw(&self, pos: BlockPos) -> u32 {
        let chunk_index = match self.get_chunk_index_for_block(pos.x, pos.z) {
            Some(idx) => idx,
            None => return 0,
        };
        let chunk = &self.chunks[chunk_index];
        chunk.get_block((pos.x & 0xF) as u32, pos.y as u32, (pos.z & 0xF) as u32)
    }

    fn get_block(&self, pos: BlockPos) -> Block {
        Block::from_id(self.get_block_raw(pos))
    }

    fn delete_block_entity(&mut self, pos: BlockPos) {
        let chunk_index = match self.get_chunk_index_for_block(pos.x, pos.z) {
            Some(idx) => idx,
            None => return,
        };
        let chunk = &mut self.chunks[chunk_index];
        chunk.delete_block_entity(BlockPos::new(pos.x & 0xF, pos.y, pos.z & 0xF));
    }

    fn get_block_entity(&self, pos: BlockPos) -> Option<&BlockEntity> {
        let chunk_index = match self.get_chunk_index_for_block(pos.x, pos.z) {
            Some(idx) => idx,
            None => return None,
        };
        let chunk = &self.chunks[chunk_index];
        chunk.get_block_entity(BlockPos::new(pos.x & 0xF, pos.y, pos.z & 0xF))
    }

    fn set_block_entity(&mut self, pos: BlockPos, block_entity: BlockEntity) {
        let chunk_index = match self.get_chunk_index_for_block(pos.x, pos.z) {
            Some(idx) => idx,
            None => return,
        };
        if let Some(nbt) = block_entity.to_nbt(true) {
            let block_entity_data = CBlockEntityData {
                x: pos.x,
                y: pos.y,
                z: pos.z,
                // For now the only nbt we send to the client is sign data
                ty: block_entity.ty(),
                nbt,
            }
            .encode();
            for player in &self.packet_senders {
                player.send_packet(&block_entity_data);
            }
        }
        let chunk = &mut self.chunks[chunk_index];
        chunk.set_block_entity(BlockPos::new(pos.x & 0xF, pos.y, pos.z & 0xF), block_entity);
    }

    fn get_chunk(&self, x: i32, z: i32) -> Option<&Chunk> {
        self.chunks.get(self.get_chunk_index_for_chunk(x, z))
    }

    fn get_chunk_mut(&mut self, x: i32, z: i32) -> Option<&mut Chunk> {
        let chunk_idx = self.get_chunk_index_for_chunk(x, z);
        self.chunks.get_mut(chunk_idx)
    }

    fn schedule_tick(&mut self, pos: BlockPos, delay: u32, priority: TickPriority) {
        self.to_be_ticked.push(TickEntry {
            pos,
            ticks_left: delay,
            tick_priority: priority,
        });
    }

    fn pending_tick_at(&mut self, pos: BlockPos) -> bool {
        self.to_be_ticked.iter().any(|e| e.pos == pos)
    }

    fn is_cursed(&self) -> bool {
        false
    }
}

impl Plot {
    fn tick(&mut self) {
        self.timings.tick();
        if self.redpiler.is_active() {
            self.redpiler.tick();
            return;
        }

        self.world
            .to_be_ticked
            .sort_by_key(|e| (e.ticks_left, e.tick_priority));
        for pending in &mut self.world.to_be_ticked {
            pending.ticks_left = pending.ticks_left.saturating_sub(1);
        }
        while self.world.to_be_ticked.first().map_or(1, |e| e.ticks_left) == 0 {
            let entry = self.world.to_be_ticked.remove(0);
            self.world
                .get_block(entry.pos)
                .tick(&mut self.world, entry.pos);
        }
    }

    /// Send a block change to all connected players
    pub fn send_block_change(&mut self, pos: BlockPos, id: u32) {
        let block_change = CBlockChange {
            block_id: id as i32,
            x: pos.x,
            y: pos.y,
            z: pos.z,
        }
        .encode();
        for player in &mut self.players {
            player.client.send_packet(&block_change);
        }
    }

    pub fn broadcast_chat_message(&mut self, message: String) {
        let broadcast_message = Message::ChatInfo(
            0,
            format!("Plot {}-{}", self.world.x, self.world.z),
            message,
        );
        self.message_sender.send(broadcast_message).unwrap();
    }

    pub fn broadcast_plot_chat_message(&mut self, message: &str) {
        for player in &mut self.players {
            player.send_chat_message(0, &ChatComponent::from_legacy_text(message));
        }
    }

    fn change_player_gamemode(&mut self, player_idx: usize, gamemode: Gamemode) {
        self.players[player_idx].set_gamemode(gamemode);
        let _ = self.message_sender.send(Message::PlayerUpdateGamemode(
            self.players[player_idx].uuid,
            gamemode,
        ));
    }

    fn on_player_move(&mut self, player_idx: usize, old: PlayerPos, new: PlayerPos) {
        let old_block = old.block_pos();
        let new_block = new.block_pos();

        if let Block::StonePressurePlate { powered: true } = self.world.get_block(old_block) {
            if !self.are_players_on_block(old_block) {
                self.set_pressure_plate(old_block, false);
            }
        }

        if let Block::StonePressurePlate { powered: false } = self.world.get_block(new_block) {
            if self.players[player_idx].on_ground {
                self.set_pressure_plate(new_block, true);
            }
        }
    }

    fn set_pressure_plate(&mut self, pos: BlockPos, powered: bool) {
        if self.redpiler.is_active() {
            self.redpiler
                .set_pressure_plate(pos, powered);
            return;
        }

        let block = self.world.get_block(pos);
        match block {
            Block::StonePressurePlate { .. } => {
                self.world
                    .set_block(pos, Block::StonePressurePlate { powered });
                Block::update_surrounding_blocks(&mut self.world, pos);
                Block::update_surrounding_blocks(&mut self.world, pos.offset(BlockFace::Bottom));
            }
            _ => warn!("Block at {} is not a pressure plate", pos),
        }
    }

    fn are_players_on_block(&mut self, pos: BlockPos) -> bool {
        for player in &self.players {
            if player.pos.block_pos() == pos && player.on_ground {
                return true;
            }
        }
        false
    }

    fn enter_plot(&mut self, player: Player) {
        self.save();
        let spawn_player = CSpawnPlayer {
            entity_id: player.entity_id as i32,
            uuid: player.uuid,
            pitch: player.pitch,
            yaw: player.yaw,
            x: player.pos.x,
            y: player.pos.y,
            z: player.pos.z,
        }
        .encode();
        let metadata_entries = vec![CEntityMetadataEntry {
            index: 17,
            metadata_type: 0,
            value: vec![player.skin_parts.bits() as u8],
        }];
        let metadata = CEntityMetadata {
            entity_id: player.entity_id as i32,
            metadata: metadata_entries,
        }
        .encode();
        for other_player in &mut self.players {
            other_player.client.send_packet(&spawn_player);
            other_player.client.send_packet(&metadata);

            let spawn_other_player = CSpawnPlayer {
                entity_id: other_player.entity_id as i32,
                uuid: other_player.uuid,
                pitch: other_player.pitch,
                yaw: other_player.yaw,
                x: other_player.pos.x,
                y: other_player.pos.y,
                z: other_player.pos.z,
            }
            .encode();
            player.client.send_packet(&spawn_other_player);

            if let Some(item) = &other_player.inventory[other_player.selected_slot as usize + 36] {
                let other_entity_equipment = CEntityEquipment {
                    entity_id: other_player.entity_id as i32,
                    equipment: vec![CEntityEquipmentEquipment {
                        slot: 0, // Main hand
                        item: Some(SlotData {
                            item_count: item.count as i8,
                            item_id: item.item_type.get_id() as i32,
                            nbt: item.nbt.clone(),
                        }),
                    }],
                }
                .encode();
                player.client.send_packet(&other_entity_equipment);
            }

            let other_metadata_entries = vec![CEntityMetadataEntry {
                index: 17,
                metadata_type: 0,
                value: vec![other_player.skin_parts.bits() as u8],
            }];
            let other_metadata = CEntityMetadata {
                entity_id: other_player.entity_id as i32,
                metadata: other_metadata_entries,
            }
            .encode();
            player.client.send_packet(&other_metadata);
        }

        if let Some(item) = &player.inventory[player.selected_slot as usize + 36] {
            let entity_equipment = CEntityEquipment {
                entity_id: player.entity_id as i32,
                equipment: vec![CEntityEquipmentEquipment {
                    slot: 0, // Main hand
                    item: Some(SlotData {
                        item_count: item.count as i8,
                        item_id: item.item_type.get_id() as i32,
                        nbt: item.nbt.clone(),
                    }),
                }],
            }
            .encode();
            for other_player in &mut self.players {
                other_player.client.send_packet(&entity_equipment);
            }
        }

        player.send_system_message(&format!(
            "Entering plot ({}, {})",
            self.world.x, self.world.z
        ));
        self.world
            .packet_senders
            .push(PlayerPacketSender::new(&player.client));
        self.scoreboard.add_player(&player);
        self.players.push(player);
        self.update_view_pos_for_player(self.players.len() - 1, true);
    }

    fn get_chunk_distance(x1: i32, z1: i32, x2: i32, z2: i32) -> u32 {
        let x = x1 - x2;
        let z = z1 - z2;
        x.abs().max(z.abs()) as u32
    }

    fn set_chunk_loaded_at_player(
        &mut self,
        player_idx: usize,
        chunk_x: i32,
        chunk_z: i32,
        was_loaded: bool,
        should_be_loaded: bool,
    ) {
        if was_loaded && !should_be_loaded {
            let unload_chunk = CUnloadChunk { chunk_x, chunk_z }.encode();
            self.players[player_idx].client.send_packet(&unload_chunk);
        } else if !was_loaded && should_be_loaded {
            if !Plot::chunk_in_plot_bounds(self.world.x, self.world.z, chunk_x, chunk_z) {
                self.players[player_idx]
                    .client
                    .send_packet(&Chunk::encode_emtpy_packet(chunk_x, chunk_z));
            } else {
                let chunk_data = self.world.chunks
                    [self.world.get_chunk_index_for_chunk(chunk_x, chunk_z)]
                .encode_packet();
                self.players[player_idx].client.send_packet(&chunk_data);
            }
        }
    }

    pub fn update_view_pos_for_player(&mut self, player_idx: usize, force_load: bool) {
        let view_distance = CONFIG.view_distance as i32;
        let (chunk_x, chunk_z) = self.players[player_idx].pos.chunk_pos();
        let last_chunk_x = self.players[player_idx].last_chunk_x;
        let last_chunk_z = self.players[player_idx].last_chunk_z;

        let update_view = CUpdateViewPosition { chunk_x, chunk_z }.encode();
        self.players[player_idx].client.send_packet(&update_view);

        if ((last_chunk_x - chunk_x).abs() <= view_distance * 2
            && (last_chunk_z - chunk_z).abs() <= view_distance * 2)
            && !force_load
        {
            let nx = chunk_x.min(last_chunk_x) - view_distance;
            let nz = chunk_z.min(last_chunk_z) - view_distance;
            let px = chunk_x.max(last_chunk_x) + view_distance;
            let pz = chunk_z.max(last_chunk_z) + view_distance;
            for x in nx..=px {
                for z in nz..=pz {
                    let was_loaded = Self::get_chunk_distance(x, z, last_chunk_x, last_chunk_z)
                        <= view_distance as u32;
                    let should_be_loaded =
                        Self::get_chunk_distance(x, z, chunk_x, chunk_z) <= view_distance as u32;
                    self.set_chunk_loaded_at_player(player_idx, x, z, was_loaded, should_be_loaded);
                }
            }
        } else {
            for x in last_chunk_x - view_distance..=last_chunk_x + view_distance {
                for z in last_chunk_z - view_distance..=last_chunk_z + view_distance {
                    self.set_chunk_loaded_at_player(player_idx, x, z, true, false);
                }
            }
            for x in chunk_x - view_distance..=chunk_x + view_distance {
                for z in chunk_z - view_distance..=chunk_z + view_distance {
                    self.set_chunk_loaded_at_player(player_idx, x, z, false, true);
                }
            }
        }
        self.players[player_idx].last_chunk_x = chunk_x;
        self.players[player_idx].last_chunk_z = chunk_z;
    }

    /// After an expensive operation or change in timings, it's important to
    /// call this function so our timings monitor doesn't think we're running
    /// behind.
    fn reset_timings(&mut self) {
        self.lag_time = Duration::ZERO;
        self.last_update_time = Instant::now();
        self.timings.reset_timings();
    }

    fn start_redpiler(&mut self, options: CompilerOptions) {
        debug!("Starting redpiler");
        let ticks = self.world.to_be_ticked.drain(..).collect();
        self.scoreboard
            .set_redpiler_state(&self.players, RedpilerState::Compiling);
        self.scoreboard
            .set_redpiler_options(&self.players, &options);
        let bounds = self.world.get_corners();
        self.redpiler
            .compile(&mut self.world, bounds, options, ticks);
        self.scoreboard
            .set_redpiler_state(&self.players, RedpilerState::Running);

        self.reset_timings();
    }

    /// Redpiler needs to reset implicitly in the case of any block changes done by a player. This can be
    fn reset_redpiler(&mut self) {
        if self.redpiler.is_active() {
            debug!("Discarding redpiler");
            let bounds = self.world.get_corners();
            self.redpiler.reset(&mut self.world, bounds);
            self.scoreboard
                .set_redpiler_state(&self.players, RedpilerState::Stopped);
            self.scoreboard
                .set_redpiler_options(&self.players, &Default::default());

            // reseting redpiler could cause a large amount of block updates
            self.reset_timings();
        }
    }

    fn destroy_entity(&mut self, entity_id: u32) {
        let destroy_entity = CDestroyEntities {
            entity_ids: vec![entity_id as i32],
        }
        .encode();
        for player in &mut self.players {
            player.client.send_packet(&destroy_entity);
        }
    }

    fn leave_plot(&mut self, uuid: u128) -> Player {
        let player_idx = self.players.iter().position(|p| p.uuid == uuid).unwrap();
        self.world.packet_senders.remove(player_idx);
        let player = self.players.remove(player_idx);

        let destroy_other_entities = CDestroyEntities {
            entity_ids: self.players.iter().map(|p| p.entity_id as i32).collect(),
        }
        .encode();
        player.client.send_packet(&destroy_other_entities);

        let chunk_offset_x = self.world.x << PLOT_SCALE;
        let chunk_offset_z = self.world.z << PLOT_SCALE;
        for chunk in &self.world.chunks {
            player.client.send_packet(
                &CUnloadChunk {
                    chunk_x: chunk_offset_x + chunk.x,
                    chunk_z: chunk_offset_z + chunk.z,
                }
                .encode(),
            );
        }
        self.destroy_entity(player.entity_id);
        self.locked_players.remove(&player.entity_id);
        self.scoreboard.remove_player(&player);
        player
    }

    fn chunk_in_plot_bounds(plot_x: i32, plot_z: i32, chunk_x: i32, chunk_z: i32) -> bool {
        let (x, z) = (chunk_x >> PLOT_SCALE, chunk_z >> PLOT_SCALE);
        plot_x == x && plot_z == z
    }

    fn in_plot_bounds(plot_x: i32, plot_z: i32, x: i32, z: i32) -> bool {
        Plot::chunk_in_plot_bounds(plot_x, plot_z, x >> 4, z >> 4)
    }

    pub fn claim_plot(&mut self, plot_x: i32, plot_z: i32, player: usize) {
        let player = &mut self.players[player];
        database::claim_plot(plot_x, plot_z, &format!("{:032x}", player.uuid));
        let center = Plot::get_center(plot_x, plot_z);
        player.teleport(PlayerPos::new(center.0, 64.0, center.1));
        player.send_system_message(&format!("Claimed plot {},{}", plot_x, plot_z));
    }

    pub fn get_center(plot_x: i32, plot_z: i32) -> (f64, f64) {
        const WIDTH: f64 = PLOT_BLOCK_WIDTH as f64;
        (
            plot_x as f64 * WIDTH + WIDTH / 2.0,
            plot_z as f64 * WIDTH + WIDTH / 2.0,
        )
    }

    pub fn get_next_plot(plot_x: i32, plot_z: i32) -> (i32, i32) {
        let x = plot_x.abs();
        let z = plot_z.abs();

        match x.cmp(&z) {
            Ordering::Greater => {
                if plot_x > 0 {
                    (plot_x, plot_z + 1)
                } else {
                    (plot_x, plot_z - 1)
                }
            }
            Ordering::Less => {
                if plot_z > 0 {
                    (plot_x - 1, plot_z)
                } else {
                    (plot_x + 1, plot_z)
                }
            }
            Ordering::Equal => {
                if plot_x == plot_z && plot_x > 0 || plot_x == x {
                    (plot_x, plot_z + 1)
                } else if plot_z == z {
                    (plot_x, plot_z - 1)
                } else {
                    (plot_x + 1, plot_z)
                }
            }
        }
    }

    fn handle_commands(&mut self) {
        let mut removal_offset = 0;
        for player_idx in 0..self.players.len() {
            let player_idx = player_idx - removal_offset;
            let commands: Vec<String> = self.players[player_idx].command_queue.drain(..).collect();
            for command in commands {
                let mut args: Vec<&str> = command.split(' ').collect();
                let command = args.remove(0);
                if self.handle_command(player_idx, command, args) {
                    removal_offset += 1;
                }
            }
        }
    }

    fn handle_messages(&mut self) {
        while let Ok(message) = self.message_receiver.try_recv() {
            match message {
                BroadcastMessage::Chat(sender, message) => {
                    for player in &mut self.players {
                        player.send_chat_message(sender, &message);
                    }
                }
                BroadcastMessage::PlayerJoinedInfo(player_join_info) => {
                    let player_info = CPlayerInfo::AddPlayer(vec![CPlayerInfoAddPlayer {
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
                    let player_info = CPlayerInfo::RemovePlayer(vec![uuid]).encode();
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
                BroadcastMessage::PlayerUpdateGamemode(uuid, gamemode) => {
                    let player_info = CPlayerInfo::UpdateGamemode(uuid, gamemode.get_id()).encode();
                    for player in &mut self.players {
                        player.client.send_packet(&player_info);
                    }
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
                        player.teleport(other.pos);
                    }
                    self.enter_plot(player);
                }
            }
        }
    }

    /// Remove players outside of the plot
    fn remove_oob_players(&mut self) {
        let mut outside_players = Vec::new();
        for player in 0..self.players.len() {
            let player = &mut self.players[player];
            if self.locked_players.contains(&player.entity_id) {
                continue;
            }
            let (plot_x, plot_z) = player.pos.plot_pos();
            if plot_x != self.world.x || plot_z != self.world.z {
                outside_players.push(player.uuid);
            }
        }

        for uuid in outside_players {
            let player = self.leave_plot(uuid);
            let player_leave_plot = Message::PlayerLeavePlot(player);
            self.message_sender.send(player_leave_plot).unwrap();
        }
    }

    /// Remove disconnected players
    fn remove_dc_players(&mut self) {
        let message_sender = &mut self.message_sender;

        let mut disconnected_players = Vec::new();
        self.players.retain(|player| {
            let alive = player.client.alive();
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
    }

    /// Update player view positions and handle packets
    fn update_players(&mut self) {
        for player_idx in 0..self.players.len() {
            if self.players[player_idx].update() {
                self.update_view_pos_for_player(player_idx, false);
            }
        }
        // Handle received packets
        for player_idx in 0..self.players.len() {
            self.handle_packets_for_player(player_idx);
        }
    }

    fn update(&mut self) {
        self.handle_messages();

        // Only tick if there are players in the plot
        if !self.players.is_empty() {
            self.timings.set_ticking(true);
            self.last_player_time = Instant::now();
            match self.tps {
                Tps::Limited(tps) if tps != 0 => {
                    let dur_per_tick = Duration::from_micros(1_000_000 / tps as u64);
                    self.lag_time += self.last_update_time.elapsed();
                    self.last_update_time = Instant::now();
                    if self.lag_time > dur_per_tick {
                        // TODO: there are some problems here: redpiler should not automatically
                        // compiler as early as it is, meaning we are not running as many ticks
                        // as we should for some reason.
                        let batch_size =
                            self.lag_time.as_micros() as u64 / dur_per_tick.as_micros() as u64;
                        // 50000 here is arbitrary. We just need a number that's not too high so we actually
                        // get around to sending block updates.
                        let batch_size = batch_size.min(50000);
                        if !self.redpiler.is_active() && self.auto_redpiler {
                            let mut ticks_completed = batch_size;
                            for i in 0..batch_size {
                                // If we're running behind, just stop right here and we can start redpiler
                                if self.timings.is_running_behind() {
                                    ticks_completed = i;
                                    break;
                                }
                                self.tick();
                            }
                            if ticks_completed > 0 {
                                self.last_nspt =
                                    Some(self.last_update_time.elapsed() / ticks_completed as u32);
                            }
                            // Check if we stopped early, and if so, start redpiler
                            if ticks_completed != batch_size {
                                self.start_redpiler(Default::default());
                            } else {
                                self.lag_time -= dur_per_tick * batch_size as u32;
                            }
                        } else {
                            // Limit the batch size to however many ticks we can fit inside the world send rate.
                            let batch_size = batch_size.min(match self.last_nspt {
                                Some(Duration::ZERO) | None => 5,
                                Some(last_nspt) => {
                                    let ticks_fit =
                                        WORLD_SEND_RATE.as_nanos() / last_nspt.as_nanos();
                                    if ticks_fit == 0 {
                                        // A tick previously took longer than the world send rate.
                                        // Run at least one just so we're not stuck doing nothing
                                        1
                                    } else {
                                        ticks_fit
                                    }
                                }
                            } as u64);
                            if batch_size != 0 {
                                // Redpiler is either already running or will not be automatically started,
                                // so there's nothing special to do here, just run the batch
                                for _ in 0..batch_size {
                                    self.tick();
                                }
                                self.lag_time -= dur_per_tick * batch_size as u32;
                                self.last_nspt =
                                    Some(self.last_update_time.elapsed() / (batch_size as u32));
                            }
                        }
                    }
                }
                Tps::Unlimited => {
                    if !self.redpiler.is_active() && self.auto_redpiler {
                        self.start_redpiler(Default::default());
                    }
                    self.last_update_time = Instant::now();
                    let batch_size = match self.last_nspt {
                        Some(Duration::ZERO) | None => 5,
                        Some(last_nspt) => {
                            let ticks_fit = WORLD_SEND_RATE.as_nanos() / last_nspt.as_nanos();
                            if ticks_fit == 0 {
                                // A tick previously took longer than the world send rate.
                                // Run at least one just so we're not stuck doing nothing
                                1
                            } else {
                                ticks_fit
                            }
                        }
                    } as u64;
                    if batch_size != 0 {
                        let batch_size = batch_size.min(50000) as u32;
                        for _ in 0..batch_size {
                            self.tick();
                        }
                        self.last_nspt = Some(self.last_update_time.elapsed() / batch_size);
                    }
                }
                _ => {}
            }

            if self.redpiler.is_active() {
                self.redpiler.flush(&mut self.world);
            }
            let now = Instant::now();
            let time_since_last_world_send = now - self.last_world_send_time;
            if time_since_last_world_send > WORLD_SEND_RATE {
                self.last_world_send_time = now;
                self.world.flush_block_changes();
            }
        } else {
            self.timings.set_ticking(false);
            // Unload plot after 600 seconds unless the plot should be always loaded
            if self.last_player_time.elapsed().as_secs() > 600 && !self.always_running {
                self.running = false;
                self.timings.stop();
            }
        }

        self.update_players();

        // Handle commands before removing players just in case they ran a command before leaving
        self.handle_commands();

        self.remove_dc_players();
        self.remove_oob_players();
    }

    fn create_async_rt() -> Runtime {
        Runtime::new().unwrap()
    }

    fn generate_chunk(layers: i32, x: i32, z: i32) -> Chunk {
        let mut chunk = Chunk::empty(x, z);

        for ry in 0..layers {
            for rx in 0..16 {
                for rz in 0..16 {
                    let block_x = (x << 4) | rx;
                    let block_z = (z << 4) | rz;

                    if block_x % PLOT_BLOCK_WIDTH == 0
                        || block_z % PLOT_BLOCK_WIDTH == 0
                        || (block_x + 1) % PLOT_BLOCK_WIDTH == 0
                        || (block_z + 1) % PLOT_BLOCK_WIDTH == 0
                    {
                        chunk.set_block(rx as u32, ry as u32, rz as u32, 4564); // Stone Bricks
                    } else {
                        chunk.set_block(rx as u32, ry as u32, rz as u32, 278); // Sandstone
                    }
                }
            }
        }
        chunk
    }

    fn from_data(
        plot_data: PlotData,
        x: i32,
        z: i32,
        rx: BusReader<BroadcastMessage>,
        tx: Sender<Message>,
        priv_rx: Receiver<PrivMessage>,
        always_running: bool,
    ) -> Plot {
        let chunk_x_offset = x << PLOT_SCALE;
        let chunk_z_offset = z << PLOT_SCALE;
        let chunks: Vec<Chunk> = plot_data
            .chunk_data
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                Chunk::load(
                    chunk_x_offset + i as i32 / PLOT_WIDTH,
                    chunk_z_offset + i as i32 % PLOT_WIDTH,
                    c,
                )
            })
            .collect();
        if chunks.len() != NUM_CHUNKS {
            error!("This plot has the wrong number of chunks!");
            let possible_scale = (chunks.len() as f64).sqrt().log2();
            error!("Note: it most likely came from a server running plot scale {}, this server is running a plot scale of {}", possible_scale, PLOT_SCALE);
        }
        let world = PlotWorld {
            x,
            z,
            chunks,
            to_be_ticked: plot_data.pending_ticks,
            packet_senders: Vec::new(),
        };
        let tps = plot_data.tps;
        Plot {
            last_player_time: Instant::now(),
            last_update_time: Instant::now(),
            last_world_send_time: Instant::now(),
            lag_time: Duration::new(0, 0),
            sleep_time: sleep_time_for_tps(tps),
            last_nspt: None,
            message_receiver: rx,
            message_sender: tx,
            priv_message_receiver: priv_rx,
            players: Vec::new(),
            locked_players: HashSet::new(),
            running: true,
            auto_redpiler: CONFIG.auto_redpiler,
            tps,
            always_running,
            redpiler: Default::default(),
            timings: TimingsMonitor::new(tps),
            owner: database::get_plot_owner(x, z).map(|s| s.parse::<HyphenatedUUID>().unwrap().0),
            async_rt: Plot::create_async_rt(),
            scoreboard: Default::default(),
            world,
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
        let plot_path = format!("./world/plots/p{},{}", x, z);
        if Path::new(&plot_path).exists() {
            let data = data::load_plot(plot_path)
                .with_context(|| format!("error loading plot {},{}", x, z))
                .unwrap();
            Plot::from_data(data, x, z, rx, tx, priv_rx, always_running)
        } else {
            Plot::from_data(data::empty_plot(), x, z, rx, tx, priv_rx, always_running)
        }
    }

    fn save(&mut self) {
        let world = &mut self.world;
        let chunk_data: Vec<ChunkData> = world.chunks.iter_mut().map(|c| c.save()).collect();
        let data = PlotData {
            tps: self.tps,
            chunk_data,
            pending_ticks: world.to_be_ticked.clone(),
        };
        data.save_to_file(format!("./world/plots/p{},{}", world.x, world.z))
            .unwrap();

        self.reset_timings();
    }

    fn run(&mut self, initial_player: Option<Player>) {
        let _guard = self.async_rt.enter();

        if let Some(player) = initial_player {
            self.enter_plot(player);
        }

        while self.running {
            // Fast path, for super high RTPS
            if self.sleep_time <= Duration::from_millis(5) && !self.players.is_empty() {
                self.update();
                if self.tps != Tps::Unlimited {
                    thread::yield_now();
                }
                continue;
            }

            let before = Instant::now();
            self.update();
            let delta = Instant::now().duration_since(before);

            if delta < self.sleep_time {
                let sleep_time = self.sleep_time - delta;
                thread::sleep(sleep_time);
            } else {
                thread::yield_now();
            }
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
        thread::Builder::new()
            .name(format!("p{},{}", x, z))
            .spawn(move || {
                let mut plot = Plot::load(x, z, rx, tx, priv_rx, always_running);
                plot.run(initial_player);
            })
            .unwrap();
    }
}

impl Drop for Plot {
    fn drop(&mut self) {
        if !self.players.is_empty() {
            for player in &mut self.players {
                player.save(); // just in case

                let world = &self.world;
                let (px, pz) = if world.x == 0 && world.z == 0 {
                    // Can't send players to spawn if spawn crashed!
                    Plot::get_center(1, 0)
                } else {
                    Plot::get_center(0, 0)
                };
                player.teleport(PlayerPos::new(px, 64.0, pz));

                player.send_raw_system_message(
                    json!({
                        "text": "The plot you were previously in has crashed!",
                        "color": "red"
                    })
                    .to_string(),
                );
            }

            while !self.players.is_empty() {
                let uuid = self.players[0].uuid;
                let player = self.leave_plot(uuid);
                self.message_sender
                    .send(Message::PlayerLeavePlot(player))
                    .unwrap();
            }
        }

        self.reset_redpiler();
        self.world
            .chunks
            .iter_mut()
            .for_each(|chunk| chunk.compress());
        self.save();
        let world = &self.world;
        self.message_sender
            .send(Message::PlotUnload(world.x, world.z))
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
