use super::Plot;

use crate::network::packets::clientbound::*;
use crate::player::{DamageSource, Gamemode};
use crate::server::Message;

impl Plot {
  pub fn player_change_gamemode(&mut self, player_idx: usize, gamemode: Gamemode) {
    self.players[player_idx].set_gamemode(gamemode);
    let _ = self.message_sender.send(Message::PlayerUpdateGamemode(
      self.players[player_idx].uuid,
      gamemode,
    ));
  }

  pub fn player_update_view_pos(&mut self, player_idx: usize, force_load: bool) {
    let view_distance = 8;
    let chunk_x = self.players[player_idx].x as i32 >> 4;
    let chunk_z = self.players[player_idx].z as i32 >> 4;
    let last_chunk_x = self.players[player_idx].last_chunk_x;
    let last_chunk_z = self.players[player_idx].last_chunk_z;

    let update_view = C40UpdateViewPosition { chunk_x, chunk_z }.encode();
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
          let was_loaded =
            Self::get_chunk_distance(x, z, last_chunk_x, last_chunk_z) <= view_distance as u32;
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

  pub fn player_respawn(&mut self, player_idx: usize) {
    self.players[player_idx].respawn();

    let teleport_packet = C56EntityTeleport {
      entity_id: self.players[player_idx].entity_id as i32,
      x: self.players[player_idx].x,
      y: self.players[player_idx].y,
      z: self.players[player_idx].z,
      yaw: self.players[player_idx].yaw,
      pitch: self.players[player_idx].pitch,
      on_ground: self.players[player_idx].on_ground,
    }
    .encode();

    for other_player in 0..self.players.len() {
      if player_idx == other_player {
        continue;
      }

      self.players[other_player]
        .client
        .send_packet(&teleport_packet);
    }
  }

  pub fn player_hurt(&mut self, player_idx: usize, source: DamageSource, amount: f32) {
    self.players[player_idx].hurt(source, amount);

    let is_dead = self.players[player_idx].health <= 0.0;

    // sounds:entity.player.death OR
    self.player_emit_sound(player_idx, if is_dead { 647 } else { match source {
      // sounds:entity.player.hurt_drown
      DamageSource::Drown => 649,
      // sounds:entity.player.hurt_on_fire
      DamageSource::InFire | DamageSource::OnFire | DamageSource::HotFloor => 650,
      // sounds:entity.player.hurt_sweet_berry_bush
      DamageSource::SweetBerryBush => 651,
      // sounds:entity.player.hurt
      _ => 648,
    }}, C50EntitySoundEffectCategories::Players, 1.0, 1.0);

    let entity_damage_status = C1AEntityStatus {
      entity_id: self.players[player_idx].entity_id as i32,
      status: match source {
        DamageSource::Drown => C1AEntityStatuses::LivingEntityDrownHurt,
        DamageSource::InFire | DamageSource::OnFire | DamageSource::HotFloor => {
          C1AEntityStatuses::LivingEntityFireHurt
        }
        DamageSource::SweetBerryBush => C1AEntityStatuses::LivingEntitySweetBerryBushHurt,
        _ => C1AEntityStatuses::LivingEntityGenericHurt,
      },
    }
    .encode();

    for other_player in 0..self.players.len() {
      if player_idx == other_player {
        continue;
      }

      self.players[other_player]
        .client
        .send_packet(&entity_damage_status);
    }

    if self.players[player_idx].health <= 0.0 {
      let entity_death_status = C1AEntityStatus {
        entity_id: self.players[player_idx].entity_id as i32,
        status: C1AEntityStatuses::LivingEntityDeath,
      }
      .encode();
      for other_player in 0..self.players.len() {
        if player_idx == other_player {
          continue;
        }
        self.players[other_player]
          .client
          .send_packet(&entity_death_status);
      }
    }
  }

  pub fn player_determin_falling(&mut self, was_on_ground: bool, player: usize) {
    if self.players[player].enable_food() {
      if self.players[player].on_ground {
        let fell = ((self.players[player].fall_distance - 3.0) * 1.0).ceil() as f32;

        if fell > 0.0 {
          let damage = fell.ceil();
          self.player_hurt(player, DamageSource::Fall, damage);
          // sounds:entity.player.big_fall and sounds:entity.player.small_fall
          self.player_emit_sound(player, if damage > 4.0 { 644 } else { 653 }, C50EntitySoundEffectCategories::Players, 1.0, 1.0);
        }
      } else if was_on_ground && !self.players[player].flying {
        if self.players[player].sprinting {
          self.players[player].exhaustion += 0.2;
        } else {
          self.players[player].exhaustion += 0.05;
        }
      }
    }
  }

  pub fn player_emit_sound(
    &mut self,
    player_idx: usize,
    sound_id: i32,
    sound_category: C50EntitySoundEffectCategories,
    volume: f32,
    pitch: f32,
  ) {
    let entity_sound_effect = C50EntitySoundEffect {
      sound_id,
      sound_category,
      entity_id: self.players[player_idx].entity_id as i32,
      volume,
      pitch,
    }
    .encode();
    for other_player in 0..self.players.len() {
      self.players[other_player]
        .client
        .send_packet(&entity_sound_effect);
    }
  }
}
