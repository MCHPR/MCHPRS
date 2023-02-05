use super::Plot;
use crate::blocks::Block;
use crate::config::CONFIG;
use crate::items::{self, UseOnBlockContext};
use crate::player::{PacketSender, PlayerPos, SkinParts};
use crate::server::Message;
use crate::utils::HyphenatedUUID;
use crate::world::World;
use mchprs_blocks::block_entities::{BlockEntity, SignBlockEntity};
use mchprs_blocks::items::{Item, ItemStack};
use mchprs_blocks::{BlockFace, BlockPos};
use mchprs_network::packets::clientbound::*;
use mchprs_network::packets::serverbound::*;
use mchprs_network::packets::SlotData;
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use tracing::error;

const ERROR_IO_ONLY: &str = "This plot cannot be interacted with while redpiler is active with `--io-only`. To stop redpiler, run `/redpiler reset`.";

impl Plot {
    pub(super) fn handle_packets_for_player(&mut self, player: usize) {
        let packets = self.players[player].client.receive_packets();
        for packet in packets {
            packet.handle(self, player);
        }
    }
}

impl ServerBoundPacketHandler for Plot {
    fn handle_tab_complete(&mut self, packet: STabComplete, player_idx: usize) {
        if !packet.text.starts_with("//load ") {
            return;
        }

        let mut path = PathBuf::from("./schems");
        if CONFIG.schemati {
            let uuid = self.players[player_idx].uuid;
            path.push(&HyphenatedUUID(uuid).to_string());
        }

        let current = &packet.text[7..];
        let mut res = CTabComplete {
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
                    let m = CTabCompleteMatch {
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

    fn handle_creative_inventory_action(
        &mut self,
        creative_inventory_action: SCreativeInventoryAction,
        player: usize,
    ) {
        if let Some(slot_data) = creative_inventory_action.clicked_item {
            if creative_inventory_action.slot < 0 || creative_inventory_action.slot >= 46 {
                return;
            }
            let item = ItemStack {
                count: slot_data.item_count as u8,
                item_type: Item::from_id(slot_data.item_id as u32),
                nbt: slot_data.nbt,
            };
            self.players[player].inventory[creative_inventory_action.slot as usize] = Some(item);
            if creative_inventory_action.slot as u32 == self.players[player].selected_slot + 36 {
                let entity_equipment = CEntityEquipment {
                    entity_id: self.players[player].entity_id as i32,
                    equipment: vec![CEntityEquipmentEquipment {
                        slot: 0, // Main hand
                        item: self.players[player].inventory
                            [creative_inventory_action.slot as usize]
                            .as_ref()
                            .map(|item| SlotData {
                                item_count: item.count as i8,
                                item_id: item.item_type.get_id() as i32,
                                nbt: item.nbt.clone(),
                            }),
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

    fn handle_animation(&mut self, animation: SAnimation, player: usize) {
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

    fn handle_player_block_placement(
        &mut self,
        player_block_placement: SPlayerBlockPlacemnt,
        player: usize,
    ) {
        let block_pos = BlockPos::new(
            player_block_placement.x,
            player_block_placement.y,
            player_block_placement.z,
        );
        let block_face = BlockFace::from_id(player_block_placement.face as u32);

        let cancel = |plot: &mut Plot| {
            plot.send_block_change(block_pos, plot.world.get_block_raw(block_pos));

            let offset_pos = block_pos.offset(block_face);
            plot.send_block_change(offset_pos, plot.world.get_block_raw(offset_pos));
        };

        let selected_slot = self.players[player].selected_slot as usize;
        let item_in_hand = if player_block_placement.hand == 0 {
            // Slot in hotbar
            self.players[player].inventory[selected_slot + 36].clone()
        } else {
            // Slot for left hand
            self.players[player].inventory[45].clone()
        };

        if !Plot::in_plot_bounds(self.world.x, self.world.z, block_pos.x, block_pos.z) {
            self.players[player].send_system_message("Can't interact with blocks outside of plot");
            cancel(self);
            return;
        }

        if let Some(item) = &item_in_hand {
            let has_permission = self.players[player].has_permission("worldedit.selection.pos");
            if item.item_type == (Item::WEWand {}) && has_permission {
                let same = self.players[player]
                    .second_position
                    .map_or(false, |p| p == block_pos);
                if !same {
                    self.players[player].worldedit_set_second_position(block_pos);
                }
                cancel(self);
                // FIXME: Because the client sends another packet after this for the left hand for most blocks,
                // redpiler will get reset anyways.
                return;
            }
        }

        if let Some(owner) = self.owner {
            let player = &mut self.players[player];
            if owner != player.uuid && !player.has_permission("plots.admin.interact.other") {
                player.send_no_permission_message();
                cancel(self);
                return;
            }
        } else if !self.players[player].has_permission("plots.admin.interact.unowned") {
            self.players[player].send_no_permission_message();
            cancel(self);
            return;
        }

        if self.redpiler.is_active() {
            let block = self.world.get_block(block_pos);
            let lever_or_button = matches!(block, Block::Lever { .. } | Block::StoneButton { .. });
            if lever_or_button && !self.players[player].crouching {
                self.redpiler.on_use_block(block_pos);
                return;
            } else {
                match self.redpiler.current_flags() {
                    Some(flags) if flags.io_only => {
                        self.players[player].send_error_message(ERROR_IO_ONLY);
                        cancel(self);
                        return;
                    }
                    _ => {}
                }
                self.reset_redpiler();
            }
        }

        if let Some(item) = item_in_hand {
            let cancelled = items::use_item_on_block(
                &item,
                &mut self.world,
                UseOnBlockContext {
                    block_face,
                    block_pos,
                    player: &mut self.players[player],
                    cursor_y: player_block_placement.cursor_y,
                },
            );
            if cancelled {
                cancel(self);
            }
            self.world.flush_block_changes();
            return;
        }

        let block = self.world.get_block(block_pos);
        if !self.players[player].crouching {
            block.on_use(&mut self.world, &mut self.players[player], block_pos, None);
            self.world.flush_block_changes();
        }
    }

    fn handle_chat_message(&mut self, chat_message: SChatMessage, player: usize) {
        let message = chat_message.message;
        if message.starts_with('/') {
            self.players[player].command_queue.push(message);
        } else {
            let player = &self.players[player];
            let broadcast_message =
                Message::ChatInfo(player.uuid, player.username.clone(), message);
            self.message_sender.send(broadcast_message).unwrap();
        }
    }

    fn handle_client_settings(&mut self, client_settings: SClientSettings, player: usize) {
        let player = &mut self.players[player];
        player.skin_parts =
            SkinParts::from_bits_truncate(client_settings.displayed_skin_parts as u32);
        let metadata_entry = CEntityMetadataEntry {
            index: 17,
            metadata_type: 0,
            value: vec![player.skin_parts.bits() as u8],
        };
        let entity_metadata = CEntityMetadata {
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

    fn handle_player_position(&mut self, player_position: SPlayerPosition, player: usize) {
        let old = self.players[player].pos;
        let new = PlayerPos::new(player_position.x, player_position.y, player_position.z);
        self.players[player].pos = new;
        self.players[player].on_ground = player_position.on_ground;
        let packet = if (new.x - old.x).abs() > 8.0
            || (new.y - old.y).abs() > 8.0
            || (new.z - old.z).abs() > 8.0
        {
            CEntityTeleport {
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
            CEntityPosition {
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

    fn handle_player_position_and_rotation(
        &mut self,
        player_position_and_rotation: SPlayerPositionAndRotation,
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
            CEntityTeleport {
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
            CEntityPositionAndRotation {
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
        let entity_head_look = CEntityHeadLook {
            entity_id: self.players[player].entity_id as i32,
            yaw: player_position_and_rotation.yaw,
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
        let entity_head_look = CEntityHeadLook {
            entity_id: self.players[player].entity_id as i32,
            yaw: player_rotation.yaw,
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

    fn handle_player_movement(&mut self, player_movement: SPlayerMovement, player: usize) {
        self.players[player].on_ground = player_movement.on_ground;
    }

    fn handle_player_digging(&mut self, player_digging: SPlayerDigging, player: usize) {
        if player_digging.status == 0 {
            let block_pos = BlockPos::new(player_digging.x, player_digging.y, player_digging.z);
            let block = self.world.get_block(block_pos);

            if !Plot::in_plot_bounds(self.world.x, self.world.z, block_pos.x, block_pos.z) {
                self.players[player].send_system_message("Can't break blocks outside of plot");
                return;
            }

            // This worldedit wand stuff should probably be done in another file. It's good enough for now.
            let item_in_hand = self.players[player].inventory
                [self.players[player].selected_slot as usize + 36]
                .clone();
            if let Some(item) = item_in_hand {
                let has_permission = self.players[player].has_permission("worldedit.selection.pos");
                if item.item_type == (Item::WEWand {}) && has_permission {
                    self.send_block_change(block_pos, block.get_id());
                    if let Some(pos) = self.players[player].first_position {
                        if pos == block_pos {
                            return;
                        }
                    }
                    self.players[player].worldedit_set_first_position(block_pos);
                    return;
                }
            }

            if let Some(owner) = self.owner {
                let player = &mut self.players[player];
                if owner != player.uuid && !player.has_permission("plots.admin.interact.other") {
                    player.send_no_permission_message();
                    self.send_block_change(block_pos, block.get_id());
                    return;
                }
            } else if !self.players[player].has_permission("plots.admin.interact.unowned") {
                self.players[player].send_no_permission_message();
                self.send_block_change(block_pos, block.get_id());
                return;
            }

            match self.redpiler.current_flags() {
                Some(flags) if flags.io_only => {
                    self.players[player].send_error_message(ERROR_IO_ONLY);
                    self.send_block_change(block_pos, block.get_id());
                    return;
                }
                _ => {}
            }

            self.reset_redpiler();

            block.destroy(&mut self.world, block_pos);
            self.world.flush_block_changes();

            let effect = CEffect {
                effect_id: 2001,
                x: player_digging.x,
                y: player_digging.y,
                z: player_digging.z,
                data: block.get_id() as i32,
                disable_relative_volume: false,
            }
            .encode();
            for other_player in 0..self.players.len() {
                if player == other_player {
                    continue;
                };
                self.players[other_player].client.send_packet(&effect);
            }
        } else {
            let selected_slot = self.players[player].selected_slot as usize + 36;
            if player_digging.status == 3 {
                self.players[player].inventory[selected_slot] = None;
            } else if player_digging.status == 4 {
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

    fn handle_entity_action(&mut self, entity_action: SEntityAction, player: usize) {
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
            CEntityMetadataEntry {
                index: 0,
                metadata_type: 0,
                value: vec![bitfield],
            },
            CEntityMetadataEntry {
                index: 6,
                metadata_type: 18,
                value: vec![if self.players[player].crouching { 5 } else { 0 }],
            },
        ];
        let entity_metadata = CEntityMetadata {
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

    fn handle_held_item_change(&mut self, held_item_change: SHeldItemChange, player: usize) {
        let entity_equipment = CEntityEquipment {
            entity_id: self.players[player].entity_id as i32,
            equipment: vec![CEntityEquipmentEquipment {
                slot: 0, // Main hand
                item: self.players[player].inventory[held_item_change.slot as usize + 36]
                    .as_ref()
                    .map(|item| SlotData {
                        item_count: item.count as i8,
                        item_id: item.item_type.get_id() as i32,
                        nbt: item.nbt.clone(),
                    }),
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
        let block_entity = BlockEntity::Sign(Box::new(SignBlockEntity {
            rows: [
                rows.next().unwrap(),
                rows.next().unwrap(),
                rows.next().unwrap(),
                rows.next().unwrap(),
            ],
        }));
        self.world.set_block_entity(pos, block_entity);
    }
}
