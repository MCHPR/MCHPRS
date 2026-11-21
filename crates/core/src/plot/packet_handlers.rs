use super::Plot;
use crate::config::CONFIG;
use crate::player::{PacketSender, PlayerPos, SkinParts};
use crate::server::Message;
use crate::utils::{self, HyphenatedUUID};
use mchprs_blocks::block_entities::{BlockEntity, SignBlockEntityText};
use mchprs_blocks::items::{Item, ItemStack};
use mchprs_blocks::BlockPos;
use mchprs_network::packets::serverbound::*;
use mchprs_network::packets::{clientbound::*, SlotData};
use mchprs_world::World;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use tracing::{error, warn};

impl Plot {
    pub(super) fn handle_packets_for_player(&mut self, player: usize) {
        let packets = self.players[player].client.receive_packets();
        for packet in packets {
            packet.handle(self, player);
        }
    }

    fn send_held_item_to_others(&mut self, player: usize) {
        let held_item =
            &self.players[player].inventory[self.players[player].selected_slot as usize + 36];
        let entity_equipment = CSetEquipment {
            entity_id: self.players[player].entity_id as i32,
            equipment: vec![CSetEquipmentEquipment {
                slot: 0, // Main hand
                item: held_item.as_ref().map(utils::encode_slot_data),
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

    pub(super) fn send_metadata_to_others(&mut self, player: usize) {
        let entity_metadata = self.players[player].metadata_packet().encode();
        for other_player in 0..self.players.len() {
            if player == other_player {
                continue;
            };
            self.players[other_player]
                .client
                .send_packet(&entity_metadata);
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
            path.push(HyphenatedUUID(uuid).to_string());
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
                    error!("{}", err);
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
        if !(0..46).contains(&creative_inventory_action.slot) {
            return;
        }
        let slot = creative_inventory_action.slot as u32;

        match creative_inventory_action.clicked_item {
            Ok(clicked_item) => {
                self.players[player].inventory[slot as usize] =
                    clicked_item.as_ref().map(item_stack_from_slot_data);
                if slot == self.players[player].selected_slot + 36 {
                    self.send_held_item_to_others(player);
                }
            }
            Err(_) => {
                self.players[player].send_error_message("The server couldn't read this item.")
            }
        }

        // We wanna retrieve the item slot from the inventory
        // rather than re-using the value from the client,
        // to avoid getting out of sync if the server validates
        // or sanitizes data or something
        let item = self.players[player].inventory[slot as usize].clone();
        self.players[player].set_inventory_slot(slot, item);
    }

    fn handle_player_abilities(&mut self, player_abilities: SPlayerAbilities, player: usize) {
        if self.players[player].flying == player_abilities.is_flying {
            return;
        }
        self.players[player].flying = player_abilities.is_flying;
        self.send_metadata_to_others(player);
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
        let packet = match relative_move(old, new) {
            Some((delta_x, delta_y, delta_z)) => CUpdateEntityPosition {
                delta_x,
                delta_y,
                delta_z,
                entity_id: self.players[player].entity_id as i32,
                on_ground: player_position.on_ground,
            }
            .encode(),
            None => CEntityPositionSync {
                entity_id: self.players[player].entity_id as i32,
                x: new.x,
                y: new.y,
                z: new.z,
                velocity_x: 0.0,
                velocity_y: 0.0,
                velocity_z: 0.0,
                yaw: self.players[player].yaw,
                pitch: self.players[player].pitch,
                on_ground: player_position.on_ground,
            }
            .encode(),
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
        let packet = match relative_move(old, new) {
            Some((delta_x, delta_y, delta_z)) => CUpdateEntityPositionAndRotation {
                delta_x,
                delta_y,
                delta_z,
                pitch: player_position_and_rotation.pitch,
                yaw: player_position_and_rotation.yaw,
                entity_id: self.players[player].entity_id as i32,
                on_ground: player_position_and_rotation.on_ground,
            }
            .encode(),
            None => CEntityPositionSync {
                entity_id: self.players[player].entity_id as i32,
                x: new.x,
                y: new.y,
                z: new.z,
                velocity_x: 0.0,
                velocity_y: 0.0,
                velocity_z: 0.0,
                yaw: self.players[player].yaw,
                pitch: self.players[player].pitch,
                on_ground: player_position_and_rotation.on_ground,
            }
            .encode(),
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
                self.send_held_item_to_others(player);
            } else if player_action.status == 4 {
                let mut stack_empty = false;
                if let Some(item_stack) = &mut self.players[player].inventory[selected_slot] {
                    item_stack.count -= 1;
                    stack_empty = item_stack.count == 0;
                }
                if stack_empty {
                    self.players[player].inventory[selected_slot] = None;
                    self.send_held_item_to_others(player);
                }
            }
        }
    }

    fn handle_player_command(&mut self, entity_action: SPlayerCommand, player: usize) {
        match entity_action.action_id {
            SPlayerCommand::START_SPRINTING => self.players[player].sprinting = true,
            SPlayerCommand::STOP_SPRINTING => self.players[player].sprinting = false,
            _ => return,
        }
        self.send_metadata_to_others(player);
    }

    fn handle_player_input(&mut self, player_input: SPlayerInput, player: usize) {
        if self.players[player].crouching == player_input.shift {
            return;
        }
        self.players[player].crouching = player_input.shift;
        self.send_metadata_to_others(player);
    }

    fn handle_set_held_item(&mut self, held_item_change: SSetHeldItem, player: usize) {
        let entity_equipment = CSetEquipment {
            entity_id: self.players[player].entity_id as i32,
            equipment: vec![CSetEquipmentEquipment {
                slot: 0, // Main hand
                item: self.players[player].inventory[held_item_change.slot as usize + 36]
                    .as_ref()
                    .map(utils::encode_slot_data),
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

    fn handle_update_sign(&mut self, packet: SUpdateSign, player: usize) {
        let pos = BlockPos::new(packet.x, packet.y, packet.z);
        let block = self.world.get_block(pos);
        if !(block.is_sign() || block.is_wall_sign()) {
            return;
        }
        if let Some(owner) = self.owner {
            let player = &mut self.players[player];
            if owner != player.uuid && !player.has_permission("plots.admin.interact.other") {
                player.send_no_permission_message();
                return;
            }
        } else if !self.players[player].has_permission("plots.admin.interact.unowned") {
            self.players[player].send_no_permission_message();
            return;
        }
        let mut rows = packet.lines.into_iter().map(Into::into);
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
        let text = SignBlockEntityText {
            messages: rows,
            ..Default::default()
        }
        .encode();
        if packet.is_front_text {
            block_entity.front_text = text;
        } else {
            block_entity.back_text = text;
        }
        self.world
            .set_block_entity(pos, BlockEntity::Sign(Box::new(block_entity)));
        self.publish_world();
    }

    fn handle_pick_item_from_block(&mut self, packet: SPickItemFromBlock, player_idx: usize) {
        let block_pos = BlockPos::new(packet.x, packet.y, packet.z);
        let block = self.world.get_block(block_pos);
        let Some(item) = block.get_item() else {
            warn!("Could not pick item from block: `{}`.", block.get_name());
            return;
        };
        let mut item_stack = ItemStack {
            item_type: item,
            count: 1,
            nbt: None,
            container_slots: Default::default(),
        };
        if packet.include_data
            && let Some(BlockEntity::Container { inventory, .. }) =
                self.world.get_block_entity(block_pos)
        {
            for entry in inventory {
                let slot = entry.slot as usize;
                if item_stack.container_slots.len() <= slot {
                    item_stack.container_slots.resize(slot + 1, None);
                }
                item_stack.container_slots[slot] = Some(entry.get_item_stack());
            }
        }

        self.players[player_idx].pick_item(item_stack);
        self.send_held_item_to_others(player_idx);
    }
}

fn item_stack_from_slot_data(slot_data: &SlotData) -> ItemStack {
    ItemStack {
        item_type: Item::from_id(slot_data.item_id as u32),
        count: slot_data.item_count as u8,
        nbt: None,
        container_slots: slot_data
            .container_items
            .iter()
            .map(|slot| {
                slot.as_ref()
                    .map(|slot_data| item_stack_from_slot_data(slot_data))
            })
            .collect(),
    }
}

// The client rounds entity positions to 1/4096 of a block with Java's `Math.round`, which rounds halves up
fn fixed_point_pos(pos: f64) -> i64 {
    let scaled = pos * 4096.0;
    let floor = scaled.floor();
    floor as i64 + i64::from(scaled - floor >= 0.5)
}

fn relative_move(old: PlayerPos, new: PlayerPos) -> Option<(i16, i16, i16)> {
    let delta = |from: f64, to: f64| {
        i16::try_from(fixed_point_pos(to).checked_sub(fixed_point_pos(from))?).ok()
    };
    Some((
        delta(old.x, new.x)?,
        delta(old.y, new.y)?,
        delta(old.z, new.z)?,
    ))
}
