use super::Plot;
use crate::config::CONFIG;
use crate::player::{PacketSender, PlayerPos, SkinParts};
use crate::server::Message;
use crate::utils::{self, HyphenatedUUID};
use mchprs_blocks::block_entities::BlockEntity;
use mchprs_blocks::items::{Item, ItemStack};
use mchprs_blocks::BlockPos;
use mchprs_network::packets::clientbound::*;
use mchprs_network::packets::serverbound::*;
use mchprs_world::World;
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use tracing::error;

impl Plot {
    pub(super) fn handle_packets_for_player(&mut self, player: usize) {
        let packets = self.players[player].client.receive_packets();
        for packet in packets {
            packet.handle(self, player);
        }
    }
}

impl ServerBoundPacketHandler for Plot {
    fn handle_command_suggestions_request(
        &mut self,
        packet: SCommandSuggestionsRequest,
        player_idx: usize,
    ) {
        if !packet.text.starts_with("//load ") {
            return;
        }

        let mut path = PathBuf::from("./schems");
        if CONFIG.schemati {
            let uuid = self.players[player_idx].uuid;
            path.push(&HyphenatedUUID(uuid).to_string());
        }

        let current = &packet.text[7..];
        let mut res = CCommandSuggestionsResponse {
            id: packet.transaction_id,
            start: 7,
            length: current.len() as i32,
            matches: Vec::new(),
        };

        let dir = match fs::read_dir(path) {
            Ok(dir) => dir,
            Err(err) => {
                if err.kind() != std::io::ErrorKind::NotFound {
                    error!("There was an error completing //load");
                    error!("{}", err.to_string());
                }
                return;
            }
        };

        for entry in dir {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with(current) {
                    let m = CCommandSuggestionsResponseMatch {
                        match_: name.to_string(),
                        tooltip: None,
                    };
                    res.matches.push(m);
                }
            }
        }

        self.players[player_idx].send_packet(&res.encode());
    }

    fn handle_keep_alive(&mut self, _keep_alive: SKeepAlive, player_idx: usize) {
        self.players[player_idx].last_keep_alive_received = Instant::now();
    }

    fn handle_set_creative_mode_slot(
        &mut self,
        creative_inventory_action: SSetCreativeModeSlot,
        player: usize,
    ) {
        if let Some(slot_data) = creative_inventory_action.clicked_item {
            if creative_inventory_action.slot < 0 || creative_inventory_action.slot >= 46 {
                return;
            }
            let item = ItemStack {
                count: slot_data.item_count as u8,
                item_type: Item::from_id(slot_data.item_id as u32),
                nbt: slot_data.nbt.map(|nbt| nbt::Blob::with_content(nbt)),
            };
            self.players[player].inventory[creative_inventory_action.slot as usize] = Some(item);
            if creative_inventory_action.slot as u32 == self.players[player].selected_slot + 36 {
                let entity_equipment = CSetEquipment {
                    entity_id: self.players[player].entity_id as i32,
                    equipment: vec![CSetEquipmentEquipment {
                        slot: 0, // Main hand
                        item: self.players[player].inventory
                            [creative_inventory_action.slot as usize]
                            .as_ref()
                            .map(|item| utils::encode_slot_data(&item)),
                    }],
                }
                .encode();
                for other_player in 0..self.players.len() {
                    if player == other_player {
                        continue;
                    };
                    self.players[other_player]
                        .client
                        .send_packet(&entity_equipment);
                }
            }
        } else {
            self.players[player].inventory[creative_inventory_action.slot as usize] = None;
        }

        // We wanna retrieve the item slot from the inventory
        // rather than re-using the value from the client,
        // to avoid getting out of sync if the server validates
        // or sanitizes data or something
        let item = self.players[player].inventory[creative_inventory_action.slot as usize].clone();
        self.players[player].set_inventory_slot(creative_inventory_action.slot as u32, item);
    }

    fn handle_player_abilities(&mut self, player_abilities: SPlayerAbilities, player: usize) {
        self.players[player].flying = player_abilities.is_flying;
    }

    fn handle_swing_arm(&mut self, animation: SSwingArm, player: usize) {
        let animation_id = match animation.hand {
            0 => 0,
            1 => 3,
            _ => 0,
        };
        let entity_animation = CEntityAnimation {
            entity_id: self.players[player].entity_id as i32,
            animation: animation_id,
        }
        .encode();
        for other_player in 0..self.players.len() {
            if player == other_player {
                continue;
            };
            self.players[other_player]
                .client
                .send_packet(&entity_animation);
        }
    }

    fn handle_use_item_on(&mut self, use_item_on: SUseItemOn, player: usize) {
        self.handle_use_item_impl(&use_item_on, player);

        let acknowledge_block_change = CAcknowledgeBlockChange {
            sequence_id: use_item_on.sequence,
        }
        .encode();
        self.players[player].send_packet(&acknowledge_block_change);
    }

    fn handle_chat_command(&mut self, chat_command: SChatCommand, player: usize) {
        self.players[player]
            .command_queue
            .push(chat_command.command);
    }

    fn handle_chat_message(&mut self, chat_message: SChatMessage, player: usize) {
        let message = chat_message.message;
        let player = &self.players[player];
        let broadcast_message = Message::ChatInfo(player.uuid, player.username.clone(), message);
        self.message_sender.send(broadcast_message).unwrap();
    }

    fn handle_client_information(&mut self, client_settings: SClientInformation, player: usize) {
        let player = &mut self.players[player];
        player.skin_parts =
            SkinParts::from_bits_truncate(client_settings.displayed_skin_parts as u32);
        let metadata_entry = CSetEntityMetadataEntry {
            index: 17,
            metadata_type: 0,
            value: vec![player.skin_parts.bits() as u8],
        };
        let entity_metadata = CSetEntityMetadata {
            entity_id: player.entity_id as i32,
            metadata: vec![metadata_entry],
        }
        .encode();
        for player in &mut self.players {
            player.client.send_packet(&entity_metadata);
        }
    }

    fn handle_plugin_message(&mut self, plugin_message: SPluginMessage, player: usize) {
        if plugin_message.channel == "worldedit:cui" {
            self.players[player].worldedit_send_cui("s|cuboid");
        }
    }

    fn handle_set_player_position(&mut self, player_position: SSetPlayerPosition, player: usize) {
        let old = self.players[player].pos;
        let new = PlayerPos::new(player_position.x, player_position.y, player_position.z);
        self.players[player].pos = new;
        self.players[player].on_ground = player_position.on_ground;
        let packet = if (new.x - old.x).abs() > 8.0
            || (new.y - old.y).abs() > 8.0
            || (new.z - old.z).abs() > 8.0
        {
            CTeleportEntity {
                entity_id: self.players[player].entity_id as i32,
                x: new.x,
                y: new.y,
                z: new.z,
                yaw: self.players[player].yaw,
                pitch: self.players[player].pitch,
                on_ground: player_position.on_ground,
            }
            .encode()
        } else {
            let delta_x = ((player_position.x * 32.0 - old.x * 32.0) * 128.0) as i16;
            let delta_y = ((player_position.y * 32.0 - old.y * 32.0) * 128.0) as i16;
            let delta_z = ((player_position.z * 32.0 - old.z * 32.0) * 128.0) as i16;
            CUpdateEntityPosition {
                delta_x,
                delta_y,
                delta_z,
                entity_id: self.players[player].entity_id as i32,
                on_ground: player_position.on_ground,
            }
            .encode()
        };
        for other_player in 0..self.players.len() {
            if player == other_player {
                continue;
            };
            self.players[other_player].client.send_packet(&packet);
        }
        self.on_player_move(player, old, new);
    }

    fn handle_set_player_position_and_rotation(
        &mut self,
        player_position_and_rotation: SSetPlayerPositionAndRotation,
        player: usize,
    ) {
        let old = self.players[player].pos;
        let new = PlayerPos::new(
            player_position_and_rotation.x,
            player_position_and_rotation.y,
            player_position_and_rotation.z,
        );
        self.players[player].pos = new;
        self.players[player].yaw = player_position_and_rotation.yaw;
        self.players[player].pitch = player_position_and_rotation.pitch;
        self.players[player].on_ground = player_position_and_rotation.on_ground;
        let packet = if (new.x - old.x).abs() > 8.0
            || (new.y - old.y).abs() > 8.0
            || (new.z - old.z).abs() > 8.0
        {
            CTeleportEntity {
                entity_id: self.players[player].entity_id as i32,
                x: new.x,
                y: new.y,
                z: new.z,
                yaw: self.players[player].yaw,
                pitch: self.players[player].pitch,
                on_ground: player_position_and_rotation.on_ground,
            }
            .encode()
        } else {
            let delta_x = ((player_position_and_rotation.x * 32.0 - old.x * 32.0) * 128.0) as i16;
            let delta_y = ((player_position_and_rotation.y * 32.0 - old.y * 32.0) * 128.0) as i16;
            let delta_z = ((player_position_and_rotation.z * 32.0 - old.z * 32.0) * 128.0) as i16;
            CUpdateEntityPositionAndRotation {
                delta_x,
                delta_y,
                delta_z,
                pitch: player_position_and_rotation.pitch,
                yaw: player_position_and_rotation.yaw,
                entity_id: self.players[player].entity_id as i32,
                on_ground: player_position_and_rotation.on_ground,
            }
            .encode()
        };
        let entity_head_look = CSetHeadRotation {
            entity_id: self.players[player].entity_id as i32,
            head_yaw: player_position_and_rotation.yaw,
        }
        .encode();
        for other_player in 0..self.players.len() {
            if player == other_player {
                continue;
            };
            self.players[other_player].client.send_packet(&packet);
            self.players[other_player]
                .client
                .send_packet(&entity_head_look);
        }
        self.on_player_move(player, old, new);
    }

    fn handle_player_rotation(&mut self, player_rotation: SPlayerRotation, player: usize) {
        self.players[player].yaw = player_rotation.yaw;
        self.players[player].pitch = player_rotation.pitch;
        self.players[player].on_ground = player_rotation.on_ground;
        let rotation_packet = CEntityRotation {
            entity_id: self.players[player].entity_id as i32,
            yaw: player_rotation.yaw,
            pitch: player_rotation.pitch,
            on_ground: player_rotation.on_ground,
        }
        .encode();
        let entity_head_look = CSetHeadRotation {
            entity_id: self.players[player].entity_id as i32,
            head_yaw: player_rotation.yaw,
        }
        .encode();
        for other_player in 0..self.players.len() {
            if player == other_player {
                continue;
            };
            self.players[other_player]
                .client
                .send_packet(&rotation_packet);
            self.players[other_player]
                .client
                .send_packet(&entity_head_look);
        }
    }

    fn handle_set_player_on_ground(&mut self, player_movement: SSetPlayerOnGround, player: usize) {
        self.players[player].on_ground = player_movement.on_ground;
    }

    fn handle_player_action(&mut self, player_action: SPlayerAction, player: usize) {
        if player_action.status == 0 {
            let block_pos = BlockPos::new(player_action.x, player_action.y, player_action.z);
            self.handle_player_digging(block_pos, player);

            let acknowledge_block_change = CAcknowledgeBlockChange {
                sequence_id: player_action.sequence,
            }
            .encode();
            self.players[player].send_packet(&acknowledge_block_change);
        } else {
            let selected_slot = self.players[player].selected_slot as usize + 36;
            if player_action.status == 3 {
                self.players[player].inventory[selected_slot] = None;
            } else if player_action.status == 4 {
                let mut stack_empty = false;
                if let Some(item_stack) = &mut self.players[player].inventory[selected_slot] {
                    item_stack.count -= 1;
                    stack_empty = item_stack.count == 0;
                }
                if stack_empty {
                    self.players[player].inventory[selected_slot] = None;
                }
            }
        }
    }

    fn handle_player_command(&mut self, entity_action: SPlayerCommand, player: usize) {
        match entity_action.action_id {
            0 => self.players[player].crouching = true,
            1 => self.players[player].crouching = false,
            3 => self.players[player].sprinting = true,
            4 => self.players[player].sprinting = false,
            _ => {}
        }
        let mut bitfield = 0;
        if self.players[player].crouching {
            bitfield |= 0x02;
        };
        if self.players[player].sprinting {
            bitfield |= 0x08;
        };
        let metadata_entries = vec![
            CSetEntityMetadataEntry {
                index: 0,
                metadata_type: 0,
                value: vec![bitfield],
            },
            CSetEntityMetadataEntry {
                index: 6,
                metadata_type: 18,
                value: vec![if self.players[player].crouching { 5 } else { 0 }],
            },
        ];
        let entity_metadata = CSetEntityMetadata {
            entity_id: self.players[player].entity_id as i32,
            metadata: metadata_entries,
        }
        .encode();
        for other_player in 0..self.players.len() {
            if player == other_player {
                continue;
            };
            self.players[other_player]
                .client
                .send_packet(&entity_metadata);
        }
    }

    fn handle_set_held_item(&mut self, held_item_change: SSetHeldItem, player: usize) {
        let entity_equipment = CSetEquipment {
            entity_id: self.players[player].entity_id as i32,
            equipment: vec![CSetEquipmentEquipment {
                slot: 0, // Main hand
                item: self.players[player].inventory[held_item_change.slot as usize + 36]
                    .as_ref()
                    .map(|item| utils::encode_slot_data(item)),
            }],
        }
        .encode();
        for other_player in 0..self.players.len() {
            if player == other_player {
                continue;
            };
            self.players[other_player]
                .client
                .send_packet(&entity_equipment);
        }
        self.players[player].selected_slot = held_item_change.slot as u32;
    }

    fn handle_update_sign(&mut self, packet: SUpdateSign, _player: usize) {
        let pos = BlockPos::new(packet.x, packet.y, packet.z);
        let mut rows = packet
            .lines
            .iter()
            .map(|line| json!({ "text": line }).to_string());
        let rows = [
            rows.next().unwrap(),
            rows.next().unwrap(),
            rows.next().unwrap(),
            rows.next().unwrap(),
        ];
        let mut block_entity = match self.world.get_block_entity(pos) {
            Some(BlockEntity::Sign(sign)) => sign.as_ref().clone(),
            _ => Default::default(),
        };
        if packet.is_front_text {
            block_entity.front_rows = rows;
        } else {
            block_entity.back_rows = rows;
        }
        self.world
            .set_block_entity(pos, BlockEntity::Sign(Box::new(block_entity)));
    }
}
