use super::scheduling::RateSchedule;
use crate::player::EntityId;
use mchprs_blocks::BlockPos;
use mchprs_network::packets::clientbound::{CBlockEntityData, CSoundEffect, ClientBoundPacket};
use mchprs_network::packets::PacketEncoder;
use mchprs_network::{PlayerConn, PlayerPacketSender};
use mchprs_save_data::plot_data::WorldSendRate;
use mchprs_world::storage::Chunk;
use rustc_hash::FxHashMap;
use std::time::{Duration, Instant};

pub struct WorldOutput {
    rate: WorldSendRate,
    schedule: RateSchedule,
    recipients: FxHashMap<EntityId, PlayerPacketSender>,
    pending_block_entities: FxHashMap<BlockPos, CBlockEntityData>,
    pending_sounds: FxHashMap<BlockPos, CSoundEffect>,
}

impl WorldOutput {
    pub fn new(rate: WorldSendRate, now: Instant) -> Self {
        Self {
            rate,
            schedule: RateSchedule::for_sends(rate, now),
            recipients: FxHashMap::default(),
            pending_block_entities: FxHashMap::default(),
            pending_sounds: FxHashMap::default(),
        }
    }

    pub fn rate(&self) -> WorldSendRate {
        self.rate
    }

    pub fn set_rate(&mut self, rate: WorldSendRate, now: Instant) {
        self.rate = rate;
        self.schedule = RateSchedule::for_sends(rate, now);
    }

    pub fn take_send_due(&mut self, now: Instant) -> bool {
        let due = self.schedule.due(now) >= 1;
        if due {
            self.schedule.complete(1);
        }
        due
    }

    pub fn send_wait(&self, now: Instant) -> Duration {
        self.schedule.wait(now)
    }

    pub fn add_player(&mut self, entity_id: EntityId, connection: &PlayerConn) {
        if self.recipients.is_empty() {
            self.pending_sounds.clear();
        }
        self.recipients
            .insert(entity_id, PlayerPacketSender::new(connection));
    }

    pub fn remove_player(&mut self, entity_id: EntityId) {
        self.recipients.remove(&entity_id);
    }

    pub fn send(&self, packet: &PacketEncoder) {
        for recipient in self.recipients.values() {
            recipient.send_packet(packet);
        }
    }

    pub fn set_block_entity(&mut self, pos: BlockPos, data: Option<CBlockEntityData>) {
        if let Some(data) = data {
            self.pending_block_entities.insert(pos, data);
        } else {
            self.pending_block_entities.remove(&pos);
        }
    }

    pub fn play_sound(
        &mut self,
        pos: BlockPos,
        sound_id: i32,
        sound_category: i32,
        volume: f32,
        pitch: f32,
    ) {
        if self.rate.0 == 0.0 {
            return;
        }
        // FIXME: Only send to players in hearing distance.
        let sound = CSoundEffect {
            sound_id: sound_id + 1,
            sound_name: None,
            has_fixed_range: None,
            range: None,
            sound_category,
            x: pos.x * 8 + 4,
            y: pos.y * 8 + 4,
            z: pos.z * 8 + 4,
            volume,
            pitch,
            // FIXME: How do we decide this?
            seed: 0,
        };
        self.pending_sounds.insert(pos, sound);
    }

    pub fn flush(&mut self, chunks: &mut [Chunk]) {
        for packet in chunks.iter_mut().flat_map(Chunk::drain_block_updates) {
            self.send(&packet.encode());
        }
        for (_, block_entity) in self.pending_block_entities.drain() {
            let encoded = block_entity.encode();
            for recipient in self.recipients.values() {
                recipient.send_packet(&encoded);
            }
        }
        for (_, sound) in self.pending_sounds.drain() {
            let encoded = sound.encode();
            for recipient in self.recipients.values() {
                recipient.send_packet(&encoded);
            }
        }
    }
}
