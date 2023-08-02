use super::*;
use crate::chat::{ChatComponentBuilder, ColorCode};
use crate::config::CONFIG;
use crate::player::PacketSender;
use crate::plot::PLOT_BLOCK_HEIGHT;
use crate::redstone;
use crate::utils::HyphenatedUUID;
use mchprs_blocks::block_entities::InventoryEntry;
use mchprs_blocks::blocks::{Block, FlipDirection, RotateAmt};
use mchprs_blocks::items::{Item, ItemStack};
use mchprs_blocks::{BlockFace, BlockFacing, BlockPos};
use mchprs_network::packets::clientbound::*;
use mchprs_network::packets::SlotData;
use once_cell::sync::Lazy;
use schematic::{load_schematic, save_schematic};
use std::time::Instant;
use tracing::error;

pub(super) fn execute_wand(ctx: CommandExecuteContext<'_>) {
    let item = ItemStack {
        count: 1,
        item_type: Item::WEWand {},
        nbt: None,
    };
    ctx.player.inventory[(ctx.player.selected_slot + 36) as usize] = Some(item);
    let entity_equipment = CEntityEquipment {
        entity_id: ctx.player.entity_id as i32,
        equipment: vec![CEntityEquipmentEquipment {
            slot: 0,
            item: ctx.player.inventory[(ctx.player.selected_slot + 36) as usize]
                .as_ref()
                .map(|item| SlotData {
                    item_count: item.count as i8,
                    item_id: item.item_type.get_id() as i32,
                    nbt: item.nbt.clone(),
                }),
        }],
    }
    .encode();
    for player in &mut ctx.plot.packet_senders {
        player.send_packet(&entity_equipment);
    }
}

pub(super) fn execute_set(ctx: CommandExecuteContext<'_>) {
    let start_time = Instant::now();
    let pattern = ctx.arguments[0].unwrap_pattern();

    let mut operation = worldedit_start_operation(ctx.player);
    capture_undo(
        ctx.plot,
        ctx.player,
        ctx.player.first_position.unwrap(),
        ctx.player.second_position.unwrap(),
    );
    for x in operation.x_range() {
        for y in operation.y_range() {
            for z in operation.z_range() {
                let block_pos = BlockPos::new(x, y, z);
                let block_id = pattern.pick().get_id();

                if ctx.plot.set_block_raw(block_pos, block_id) {
                    operation.update_block();
                }
            }
        }
    }

    let blocks_updated = operation.blocks_updated();

    ctx.player.send_worldedit_message(&format!(
        "Operation completed: {} block(s) affected ({:?})",
        blocks_updated,
        start_time.elapsed()
    ));
}

pub(super) fn execute_replace(ctx: CommandExecuteContext<'_>) {
    let start_time = Instant::now();

    let filter = ctx.arguments[0].unwrap_mask();
    let pattern = ctx.arguments[1].unwrap_pattern();

    let mut operation = worldedit_start_operation(ctx.player);
    capture_undo(
        ctx.plot,
        ctx.player,
        ctx.player.first_position.unwrap(),
        ctx.player.second_position.unwrap(),
    );
    for x in operation.x_range() {
        for y in operation.y_range() {
            for z in operation.z_range() {
                let block_pos = BlockPos::new(x, y, z);

                if filter.matches(ctx.plot.get_block(block_pos)) {
                    let block_id = pattern.pick().get_id();

                    if ctx.plot.set_block_raw(block_pos, block_id) {
                        operation.update_block();
                    }
                }
            }
        }
    }

    let blocks_updated = operation.blocks_updated();

    ctx.player.send_worldedit_message(&format!(
        "Operation completed: {} block(s) affected ({:?})",
        blocks_updated,
        start_time.elapsed()
    ));
}

pub(super) fn execute_count(ctx: CommandExecuteContext<'_>) {
    let start_time = Instant::now();

    let filter = ctx.arguments[0].unwrap_pattern();

    let mut blocks_counted = 0;
    let operation = worldedit_start_operation(ctx.player);
    for x in operation.x_range() {
        for y in operation.y_range() {
            for z in operation.z_range() {
                let block_pos = BlockPos::new(x, y, z);
                if filter.matches(ctx.plot.get_block(block_pos)) {
                    blocks_counted += 1;
                }
            }
        }
    }

    ctx.player.send_worldedit_message(&format!(
        "Counted {} block(s) ({:?})",
        blocks_counted,
        start_time.elapsed()
    ));
}

pub(super) fn execute_copy(ctx: CommandExecuteContext<'_>) {
    let start_time = Instant::now();

    let origin = ctx.player.pos.block_pos();
    let clipboard = create_clipboard(
        ctx.plot,
        origin,
        ctx.player.first_position.unwrap(),
        ctx.player.second_position.unwrap(),
    );
    ctx.player.worldedit_clipboard = Some(clipboard);

    ctx.player.send_worldedit_message(&format!(
        "Your selection was copied. ({:?})",
        start_time.elapsed()
    ));
}

pub(super) fn execute_cut(ctx: CommandExecuteContext<'_>) {
    let start_time = Instant::now();

    let first_pos = ctx.player.first_position.unwrap();
    let second_pos = ctx.player.second_position.unwrap();

    capture_undo(ctx.plot, ctx.player, first_pos, second_pos);

    let origin = ctx.player.pos.block_pos();
    let clipboard = create_clipboard(ctx.plot, origin, first_pos, second_pos);
    ctx.player.worldedit_clipboard = Some(clipboard);
    clear_area(ctx.plot, first_pos, second_pos);

    ctx.player.send_worldedit_message(&format!(
        "Your selection was cut. ({:?})",
        start_time.elapsed()
    ));
}

pub(super) fn execute_move(mut ctx: CommandExecuteContext<'_>) {
    let start_time = Instant::now();

    let move_amt = ctx.arguments[0].unwrap_uint();
    let direction = ctx.arguments[1].unwrap_direction();

    let first_pos = ctx.player.first_position.unwrap();
    let second_pos = ctx.player.second_position.unwrap();

    let zero_pos = BlockPos::new(0, 0, 0);

    let undo = WorldEditUndo {
        clipboards: vec![
            create_clipboard(ctx.plot, first_pos.min(second_pos), first_pos, second_pos),
            create_clipboard(
                ctx.plot,
                first_pos.min(second_pos),
                direction.offset_pos(first_pos, move_amt as i32),
                direction.offset_pos(second_pos, move_amt as i32),
            ),
        ],
        pos: first_pos.min(second_pos),
        plot_x: ctx.plot.x,
        plot_z: ctx.plot.z,
    };
    ctx.player.worldedit_undo.push(undo);

    let clipboard = create_clipboard(ctx.plot, zero_pos, first_pos, second_pos);
    clear_area(ctx.plot, first_pos, second_pos);
    paste_clipboard(
        ctx.plot,
        &clipboard,
        direction.offset_pos(zero_pos, move_amt as i32),
        ctx.has_flag('a'),
    );

    if ctx.has_flag('s') {
        let first_pos = direction.offset_pos(first_pos, move_amt as i32);
        let second_pos = direction.offset_pos(second_pos, move_amt as i32);
        let player = &mut ctx.player;
        player.worldedit_set_first_position(first_pos);
        player.worldedit_set_second_position(second_pos);
    }

    ctx.player.send_worldedit_message(&format!(
        "Your selection was moved. ({:?})",
        start_time.elapsed()
    ));
}

pub(super) fn execute_paste(ctx: CommandExecuteContext<'_>) {
    let start_time = Instant::now();

    if ctx.player.worldedit_clipboard.is_some() {
        // Here I am cloning the clipboard. This is bad. Don't do this.
        let cb = &ctx.player.worldedit_clipboard.clone().unwrap();
        let pos = ctx.player.pos.block_pos();
        let offset_x = pos.x - cb.offset_x;
        let offset_y = pos.y - cb.offset_y;
        let offset_z = pos.z - cb.offset_z;
        capture_undo(
            ctx.plot,
            ctx.player,
            BlockPos::new(offset_x, offset_y, offset_z),
            BlockPos::new(
                offset_x + cb.size_x as i32,
                offset_y + cb.size_y as i32,
                offset_z + cb.size_z as i32,
            ),
        );
        paste_clipboard(ctx.plot, cb, pos, ctx.has_flag('a'));
        ctx.player.send_worldedit_message(&format!(
            "Your clipboard was pasted. ({:?})",
            start_time.elapsed()
        ));
    } else {
        ctx.player.send_system_message("Your clipboard is empty!");
    }
}

static SCHEMATI_VALIDATE_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[a-zA-Z0-9_.]+\.schem(atic)?").unwrap());

pub(super) fn execute_load(ctx: CommandExecuteContext<'_>) {
    let start_time = Instant::now();

    let mut file_name = ctx.arguments[0].unwrap_string().clone();
    if !SCHEMATI_VALIDATE_REGEX.is_match(&file_name) {
        ctx.player.send_error_message("Filename is invalid");
        return;
    }

    if CONFIG.schemati {
        let prefix = HyphenatedUUID(ctx.player.uuid).to_string() + "/";
        file_name.insert_str(0, &prefix);
    }

    let clipboard = load_schematic(&file_name);
    match clipboard {
        Ok(cb) => {
            ctx.player.worldedit_clipboard = Some(cb);
            ctx.player.send_worldedit_message(&format!(
                "The schematic was loaded to your clipboard. Do //paste to birth it into the world. ({:?})",
                start_time.elapsed()
            ));
        }
        Err(e) => {
            if let Some(e) = e.downcast_ref::<std::io::Error>() {
                if e.kind() == std::io::ErrorKind::NotFound {
                    let msg = "The specified schematic file could not be found.";
                    ctx.player.send_error_message(msg);
                    return;
                }
            }
            error!("There was an error loading a schematic:");
            error!("{}", e);
            ctx.player.send_error_message(
                "There was an error loading the schematic. Check console for more details.",
            );
        }
    }
}

pub(super) fn execute_save(ctx: CommandExecuteContext<'_>) {
    let start_time = Instant::now();

    let mut file_name = ctx.arguments[0].unwrap_string().clone();
    if !SCHEMATI_VALIDATE_REGEX.is_match(&file_name) {
        ctx.player.send_error_message("Filename is invalid");
        return;
    }

    if CONFIG.schemati {
        let prefix = HyphenatedUUID(ctx.player.uuid).to_string() + "/";
        file_name.insert_str(0, &prefix);
    }

    let clipboard = ctx.player.worldedit_clipboard.as_ref().unwrap();
    match save_schematic(&file_name, clipboard) {
        Ok(_) => {
            ctx.player.send_worldedit_message(&format!(
                "The schematic was saved sucessfuly. ({:?})",
                start_time.elapsed()
            ));
        }
        Err(err) => {
            error!("There was an error saving a schematic: ");
            error!("{:?}", err);
            ctx.player
                .send_error_message("There was an error saving the schematic.");
        }
    }
}

pub(super) fn execute_stack(ctx: CommandExecuteContext<'_>) {
    let start_time = Instant::now();

    let stack_amt = ctx.arguments[0].unwrap_uint();
    let direction = ctx.arguments[1].unwrap_direction();
    let pos1 = ctx.player.first_position.unwrap();
    let pos2 = ctx.player.second_position.unwrap();
    let clipboard = create_clipboard(ctx.plot, pos1, pos1, pos2);
    let stack_offset = match direction {
        BlockFacing::North | BlockFacing::South => clipboard.size_z,
        BlockFacing::East | BlockFacing::West => clipboard.size_x,
        BlockFacing::Up | BlockFacing::Down => clipboard.size_y,
    };
    let mut undo_cbs = Vec::new();
    for i in 1..stack_amt + 1 {
        let offset = (i * stack_offset) as i32;
        let block_pos = direction.offset_pos(pos1, offset);
        undo_cbs.push(create_clipboard(
            ctx.plot,
            pos1,
            block_pos,
            direction.offset_pos(pos2, offset),
        ));
        paste_clipboard(ctx.plot, &clipboard, block_pos, ctx.has_flag('a'));
    }
    let undo = WorldEditUndo {
        clipboards: undo_cbs,
        pos: pos1,
        plot_x: ctx.plot.x,
        plot_z: ctx.plot.z,
    };
    ctx.player.worldedit_undo.push(undo);

    ctx.player.send_worldedit_message(&format!(
        "Your selection was stacked. ({:?})",
        start_time.elapsed()
    ));
}

pub(super) fn execute_undo(ctx: CommandExecuteContext<'_>) {
    if ctx.player.worldedit_undo.is_empty() {
        ctx.player
            .send_error_message("There is nothing left to undo.");
        return;
    }
    let undo = ctx.player.worldedit_undo.pop().unwrap();
    if undo.plot_x != ctx.plot.x || undo.plot_z != ctx.plot.z {
        ctx.player
            .send_error_message("Cannot undo outside of your current plot.");
        return;
    }
    let redo = WorldEditUndo {
        clipboards: undo
            .clipboards
            .iter()
            .map(|clipboard| {
                let first_pos = BlockPos {
                    x: undo.pos.x - clipboard.offset_x,
                    y: undo.pos.y - clipboard.offset_y,
                    z: undo.pos.z - clipboard.offset_z,
                };
                let second_pos = BlockPos {
                    x: first_pos.x + clipboard.size_x as i32 - 1,
                    y: first_pos.y + clipboard.size_y as i32 - 1,
                    z: first_pos.z + clipboard.size_z as i32 - 1,
                };
                create_clipboard(ctx.plot, undo.pos, first_pos, second_pos)
            })
            .collect(),
        ..undo
    };
    for clipboard in &undo.clipboards {
        paste_clipboard(ctx.plot, clipboard, undo.pos, false);
    }
    ctx.player.worldedit_redo.push(redo);
}

pub(super) fn execute_redo(ctx: CommandExecuteContext<'_>) {
    if ctx.player.worldedit_redo.is_empty() {
        ctx.player
            .send_error_message("There is nothing left to redo.");
        return;
    }
    let redo = ctx.player.worldedit_redo.pop().unwrap();
    if redo.plot_x != ctx.plot.x || redo.plot_z != ctx.plot.z {
        ctx.player
            .send_error_message("Cannot redo outside of your current plot.");
        return;
    }
    let undo = WorldEditUndo {
        clipboards: redo
            .clipboards
            .iter()
            .map(|clipboard| {
                let first_pos = BlockPos {
                    x: redo.pos.x - clipboard.offset_x,
                    y: redo.pos.y - clipboard.offset_y,
                    z: redo.pos.z - clipboard.offset_z,
                };
                let second_pos = BlockPos {
                    x: first_pos.x + clipboard.size_x as i32 - 1,
                    y: first_pos.y + clipboard.size_y as i32 - 1,
                    z: first_pos.z + clipboard.size_z as i32 - 1,
                };
                create_clipboard(ctx.plot, redo.pos, first_pos, second_pos)
            })
            .collect(),
        ..redo
    };
    for clipboard in &redo.clipboards {
        paste_clipboard(ctx.plot, clipboard, redo.pos, false);
    }
    ctx.player.worldedit_undo.push(undo);
}

pub(super) fn execute_sel(ctx: CommandExecuteContext<'_>) {
    let player = ctx.player;
    player.first_position = None;
    player.second_position = None;
    player.send_worldedit_message("Selection cleared.");
    player.worldedit_send_cui("s|cuboid");
}

pub(super) fn execute_pos1(ctx: CommandExecuteContext<'_>) {
    let pos = ctx.player.pos.block_pos();
    ctx.player.worldedit_set_first_position(pos);
}

pub(super) fn execute_pos2(ctx: CommandExecuteContext<'_>) {
    let pos = ctx.player.pos.block_pos();
    ctx.player.worldedit_set_second_position(pos);
}

pub(super) fn execute_hpos1(mut ctx: CommandExecuteContext<'_>) {
    let player = &mut ctx.player;
    let pitch = player.pitch as f64;
    let yaw = player.yaw as f64;

    let result = ray_trace_block(ctx.plot, player.pos, pitch, yaw, 300.0);

    let player = ctx.player;
    match result {
        Some(pos) => player.worldedit_set_first_position(pos),
        None => player.send_error_message("No block in sight!"),
    }
}

pub(super) fn execute_hpos2(mut ctx: CommandExecuteContext<'_>) {
    let player = &mut ctx.player;
    let pitch = player.pitch as f64;
    let yaw = player.yaw as f64;

    let result = ray_trace_block(ctx.plot, player.pos, pitch, yaw, 300.0);

    let player = &mut ctx.player;
    match result {
        Some(pos) => player.worldedit_set_second_position(pos),
        None => player.send_error_message("No block in sight!"),
    }
}

pub(super) fn execute_expand(ctx: CommandExecuteContext<'_>) {
    let amount = ctx.arguments[0].unwrap_uint();
    let direction = ctx.arguments[1].unwrap_direction();
    let player = ctx.player;

    expand_selection(
        player,
        direction.offset_pos(BlockPos::zero(), amount as i32),
        false,
    );

    player.send_worldedit_message(&format!("Region expanded {} block(s).", amount));
}

pub(super) fn execute_contract(ctx: CommandExecuteContext<'_>) {
    let amount = ctx.arguments[0].unwrap_uint();
    let direction = ctx.arguments[1].unwrap_direction();
    let player = ctx.player;

    expand_selection(
        player,
        direction.offset_pos(BlockPos::zero(), amount as i32),
        true,
    );

    player.send_worldedit_message(&format!("Region contracted {} block(s).", amount));
}

pub(super) fn execute_shift(ctx: CommandExecuteContext<'_>) {
    let amount = ctx.arguments[0].unwrap_uint();
    let direction = ctx.arguments[1].unwrap_direction();
    let player = ctx.player;
    let first_pos = player.first_position.unwrap();
    let second_pos = player.second_position.unwrap();

    let mut move_both_points = |x, y, z| {
        player.worldedit_set_first_position(BlockPos::new(
            first_pos.x + x,
            first_pos.y + y,
            first_pos.z + z,
        ));
        player.worldedit_set_second_position(BlockPos::new(
            second_pos.x + x,
            second_pos.y + y,
            second_pos.z + z,
        ));
    };

    match direction {
        BlockFacing::Up => move_both_points(0, amount as i32, 0),
        BlockFacing::Down => move_both_points(0, -(amount as i32), 0),
        BlockFacing::East => move_both_points(amount as i32, 0, 0),
        BlockFacing::West => move_both_points(-(amount as i32), 0, 0),
        BlockFacing::South => move_both_points(0, 0, amount as i32),
        BlockFacing::North => move_both_points(0, 0, -(amount as i32)),
    }

    player.send_worldedit_message(&format!("Region shifted {} block(s).", amount));
}

pub(super) fn execute_flip(ctx: CommandExecuteContext<'_>) {
    let start_time = Instant::now();

    let direction = ctx.arguments[0].unwrap_direction();
    let clipboard = ctx.player.worldedit_clipboard.as_ref().unwrap();
    let size_x = clipboard.size_x;
    let size_y = clipboard.size_y;
    let size_z = clipboard.size_z;
    let volume = size_x * size_y * size_z;

    let flip_pos = |mut pos: BlockPos| {
        match direction {
            BlockFacing::East | BlockFacing::West => pos.x = size_x as i32 - 1 - pos.x,
            BlockFacing::North | BlockFacing::South => pos.z = size_z as i32 - 1 - pos.z,
            BlockFacing::Up | BlockFacing::Down => pos.y = size_y as i32 - 1 - pos.y,
        }
        pos
    };

    let mut newcpdata = PalettedBitBuffer::new((volume) as usize, 9);

    let mut c_x = 0;
    let mut c_y = 0;
    let mut c_z = 0;
    for i in 0..volume {
        let BlockPos {
            x: n_x,
            y: n_y,
            z: n_z,
        } = flip_pos(BlockPos::new(c_x, c_y, c_z));
        let n_i = (n_y as u32 * size_x * size_z) + (n_z as u32 * size_x) + n_x as u32;

        let mut block = Block::from_id(clipboard.data.get_entry(i as usize));
        match direction {
            BlockFacing::East | BlockFacing::West => block.flip(FlipDirection::FlipX),
            BlockFacing::North | BlockFacing::South => block.flip(FlipDirection::FlipZ),
            _ => {}
        }
        newcpdata.set_entry(n_i as usize, block.get_id());

        // Ok now lets increment the coordinates for the next block
        c_x += 1;

        if c_x as u32 == size_x {
            c_x = 0;
            c_z += 1;

            if c_z as u32 == size_z {
                c_z = 0;
                c_y += 1;
            }
        }
    }

    let offset = flip_pos(BlockPos::new(
        clipboard.offset_x,
        clipboard.offset_y,
        clipboard.offset_z,
    ));
    let cb = WorldEditClipboard {
        offset_x: offset.x,
        offset_y: offset.y,
        offset_z: offset.z,
        size_x,
        size_y,
        size_z,
        data: newcpdata,
        block_entities: clipboard
            .block_entities
            .iter()
            .map(|(pos, e)| (flip_pos(*pos), e.clone()))
            .collect(),
    };

    ctx.player.worldedit_clipboard = Some(cb);
    ctx.player.send_worldedit_message(&format!(
        "The clipboard copy has been flipped. ({:?})",
        start_time.elapsed()
    ));
}

pub(super) fn execute_rotate(ctx: CommandExecuteContext<'_>) {
    let start_time = Instant::now();
    let rotate_amt = ctx.arguments[0].unwrap_uint();
    let rotate_amt = match rotate_amt % 360 {
        0 => {
            ctx.player
                .send_worldedit_message("Successfully rotated by 0! That took a lot of work.");
            return;
        }
        90 => RotateAmt::Rotate90,
        180 => RotateAmt::Rotate180,
        270 => RotateAmt::Rotate270,
        _ => {
            ctx.player
                .send_error_message("Rotate amount must be a multiple of 90.");
            return;
        }
    };

    let clipboard = ctx.player.worldedit_clipboard.as_ref().unwrap();
    let size_x = clipboard.size_x;
    let size_y = clipboard.size_y;
    let size_z = clipboard.size_z;
    let volume = size_x * size_y * size_z;

    let (n_size_x, n_size_z) = match rotate_amt {
        RotateAmt::Rotate90 | RotateAmt::Rotate270 => (size_z, size_x),
        _ => (size_x, size_z),
    };

    let rotate_pos = |pos: BlockPos| match rotate_amt {
        RotateAmt::Rotate90 => BlockPos {
            x: n_size_x as i32 - 1 - pos.z,
            y: pos.y,
            z: pos.x,
        },
        RotateAmt::Rotate180 => BlockPos {
            x: n_size_x as i32 - 1 - pos.x,
            y: pos.y,
            z: n_size_z as i32 - 1 - pos.z,
        },
        RotateAmt::Rotate270 => BlockPos {
            x: pos.z,
            y: pos.y,
            z: n_size_z as i32 - 1 - pos.x,
        },
    };

    let mut newcpdata = PalettedBitBuffer::new((volume) as usize, 9);

    let mut c_x = 0;
    let mut c_y = 0;
    let mut c_z = 0;
    for i in 0..volume {
        let BlockPos {
            x: n_x,
            y: n_y,
            z: n_z,
        } = rotate_pos(BlockPos::new(c_x, c_y, c_z));
        let n_i = (n_y as u32 * n_size_x * n_size_z) + (n_z as u32 * n_size_x) + n_x as u32;

        let mut block = Block::from_id(clipboard.data.get_entry(i as usize));
        block.rotate(rotate_amt);
        newcpdata.set_entry(n_i as usize, block.get_id());

        // Ok now lets increment the coordinates for the next block
        c_x += 1;

        if c_x as u32 == size_x {
            c_x = 0;
            c_z += 1;

            if c_z as u32 == size_z {
                c_z = 0;
                c_y += 1;
            }
        }
    }

    let offset = rotate_pos(BlockPos::new(
        clipboard.offset_x,
        clipboard.offset_y,
        clipboard.offset_z,
    ));
    let cb = WorldEditClipboard {
        offset_x: offset.x,
        offset_y: offset.y,
        offset_z: offset.z,
        size_x: n_size_x,
        size_y,
        size_z: n_size_z,
        data: newcpdata,
        block_entities: clipboard
            .block_entities
            .iter()
            .map(|(pos, e)| (rotate_pos(*pos), e.clone()))
            .collect(),
    };

    ctx.player.worldedit_clipboard = Some(cb);
    ctx.player.send_worldedit_message(&format!(
        "The clipboard copy has been rotated. ({:?})",
        start_time.elapsed()
    ));
}

pub(super) fn execute_help(mut ctx: CommandExecuteContext<'_>) {
    let command_name = ctx.arguments[0].unwrap_string().clone();
    let slash_command_name = "/".to_owned() + &command_name;
    let player = &mut ctx.player;

    let maybe_command = COMMANDS
        .get(command_name.as_str())
        .or_else(|| COMMANDS.get(slash_command_name.as_str()));
    let command = match maybe_command {
        Some(command) => command,
        None => {
            player.send_error_message(&format!("Unknown command: {}", command_name));
            return;
        }
    };

    let mut message = vec![
        ChatComponentBuilder::new("--------------".to_owned())
            .color_code(ColorCode::Yellow)
            .strikethrough(true)
            .finish(),
        ChatComponentBuilder::new(format!(" Help for /{} ", command_name)).finish(),
        ChatComponentBuilder::new("--------------\n".to_owned())
            .color_code(ColorCode::Yellow)
            .strikethrough(true)
            .finish(),
        ChatComponentBuilder::new(command.description.to_owned())
            .color_code(ColorCode::Gray)
            .finish(),
        ChatComponentBuilder::new("\nUsage: ".to_owned())
            .color_code(ColorCode::Gray)
            .finish(),
        ChatComponentBuilder::new(format!("/{}", command_name))
            .color_code(ColorCode::Gold)
            .finish(),
    ];

    for arg in command.arguments {
        message.append(&mut vec![
            ChatComponentBuilder::new(" [".to_owned())
                .color_code(ColorCode::Yellow)
                .finish(),
            ChatComponentBuilder::new(arg.name.to_owned())
                .color_code(ColorCode::Gold)
                .finish(),
            ChatComponentBuilder::new("]".to_owned())
                .color_code(ColorCode::Yellow)
                .finish(),
        ]);
    }

    message.push(
        ChatComponentBuilder::new("\nArguments:".to_owned())
            .color_code(ColorCode::Gray)
            .finish(),
    );

    for arg in command.arguments {
        message.append(&mut vec![
            ChatComponentBuilder::new("\n  [".to_owned())
                .color_code(ColorCode::Yellow)
                .finish(),
            ChatComponentBuilder::new(arg.name.to_owned())
                .color_code(ColorCode::Gold)
                .finish(),
            ChatComponentBuilder::new("]".to_owned())
                .color_code(ColorCode::Yellow)
                .finish(),
        ]);

        let default = if let Some(arg) = &arg.default {
            match arg {
                Argument::UnsignedInteger(int) => Some(int.to_string()),
                _ => None,
            }
        } else {
            match arg.argument_type {
                ArgumentType::Direction | ArgumentType::DirectionVector => Some("me".to_string()),
                ArgumentType::UnsignedInteger => Some("1".to_string()),
                _ => None,
            }
        };
        if let Some(default) = default {
            message.push(
                ChatComponentBuilder::new(format!(" (defaults to {})", default))
                    .color_code(ColorCode::Gray)
                    .finish(),
            );
        }

        message.push(
            ChatComponentBuilder::new(format!(": {}", arg.description))
                .color_code(ColorCode::Gray)
                .finish(),
        );
    }

    if !command.flags.is_empty() {
        message.push(
            ChatComponentBuilder::new("\nFlags:".to_owned())
                .color_code(ColorCode::Gray)
                .finish(),
        );

        for flag in command.flags {
            message.append(&mut vec![
                ChatComponentBuilder::new(format!("\n  -{}", flag.letter))
                    .color_code(ColorCode::Gold)
                    .finish(),
                ChatComponentBuilder::new(format!(": {}", flag.description))
                    .color_code(ColorCode::Gray)
                    .finish(),
            ]);
        }
    }

    player.send_chat_message(0, &message);
}

pub(super) fn execute_up(ctx: CommandExecuteContext<'_>) {
    let distance = ctx.arguments[0].unwrap_uint();
    let player = ctx.player;

    let mut pos = player.pos;
    pos.y += distance as f64;
    let block_pos = pos.block_pos();

    let platform_pos = block_pos.offset(BlockFace::Bottom);
    if matches!(ctx.plot.get_block(platform_pos), Block::Air {}) {
        ctx.plot.set_block(platform_pos, Block::Glass {});
    }

    player.teleport(pos);
}

pub(super) fn execute_ascend(ctx: CommandExecuteContext<'_>) {
    let initial_levels = ctx.arguments[0].unwrap_uint();
    let mut levels = initial_levels;

    let player = ctx.player;
    let player_pos = player.pos.block_pos();
    let mut player_y = player_pos.y;

    for (y, _) in (player_y..=PLOT_BLOCK_HEIGHT).enumerate() {
        if levels == 0 {
            break;
        }
        let y = y as i32 + 1;

        let floor_pos = player_pos + BlockPos::new(0, y - 1, 0);
        let pos = player_pos + BlockPos::new(0, y, 0);
        let high_pos = player_pos + BlockPos::new(0, y + 1, 0);
        if ctx.plot.get_block(floor_pos) != (Block::Air {})
            && ctx.plot.get_block(pos) == (Block::Air {})
            && ctx.plot.get_block(high_pos) == (Block::Air {})
        {
            player_y = pos.y;
            levels -= 1;
        }
    }

    if player_y == player_pos.y {
        player.send_error_message("No free spot above you found.");
    } else {
        let mut pos = player.pos;
        pos.y = player_y as f64;
        player.teleport(pos);
        player.send_worldedit_message(&format!("Ascended {} levels.", initial_levels - levels));
    }
}

pub(super) fn execute_descend(ctx: CommandExecuteContext<'_>) {
    let initial_levels = ctx.arguments[0].unwrap_uint();
    let mut levels = initial_levels;

    let player = ctx.player;
    let player_pos = player.pos.block_pos();
    let mut player_y = player_pos.y;

    for (y, _) in (1..player_y).enumerate() {
        if levels == 0 {
            break;
        }
        let y = -(y as i32 + 1);

        let floor_pos = player_pos + BlockPos::new(0, y - 1, 0);
        let pos = player_pos + BlockPos::new(0, y, 0);
        let high_pos = player_pos + BlockPos::new(0, y + 1, 0);
        if ctx.plot.get_block(floor_pos) != (Block::Air {})
            && ctx.plot.get_block(pos) == (Block::Air {})
            && ctx.plot.get_block(high_pos) == (Block::Air {})
        {
            player_y = pos.y;
            levels -= 1;
        }
    }

    if player_y == player_pos.y {
        player.send_error_message("No free spot below you found.");
    } else {
        let mut pos = player.pos;
        pos.y = player_y as f64;
        player.teleport(pos);
        player.send_worldedit_message(&format!("Descended {} levels.", initial_levels - levels));
    }
}

pub(super) fn execute_rstack(ctx: CommandExecuteContext<'_>) {
    let start_time = Instant::now();

    let stack_amt = ctx.arguments[0].unwrap_uint();
    let stack_spacing = ctx.arguments[1].unwrap_uint();
    let direction = ctx.arguments[2].unwrap_direction_vec();
    let pos1 = ctx.player.first_position.unwrap();
    let pos2 = ctx.player.second_position.unwrap();
    let clipboard = create_clipboard(ctx.plot, pos1, pos1, pos2);
    let mut undo_cbs = Vec::new();
    for i in 1..stack_amt + 1 {
        let offset = (i * stack_spacing) as i32;

        let block_pos = pos1 + direction * offset;
        undo_cbs.push(create_clipboard(
            ctx.plot,
            pos1,
            block_pos,
            pos2 + direction * offset,
        ));
        paste_clipboard(ctx.plot, &clipboard, block_pos, !ctx.has_flag('a'));
    }
    let undo = WorldEditUndo {
        clipboards: undo_cbs,
        pos: pos1,
        plot_x: ctx.plot.x,
        plot_z: ctx.plot.z,
    };

    if ctx.has_flag('e') {
        expand_selection(
            ctx.player,
            direction * (stack_amt * stack_spacing) as i32,
            false,
        );
    }

    let player = ctx.player;
    player.worldedit_undo.push(undo);

    player.send_worldedit_message(&format!(
        "Your selection was stacked successfully. ({:?})",
        start_time.elapsed()
    ));
}

pub(super) fn execute_update(ctx: CommandExecuteContext<'_>) {
    let start_time = Instant::now();

    let operation = worldedit_start_operation(ctx.player);
    for x in operation.x_range() {
        for y in operation.y_range() {
            for z in operation.z_range() {
                let block_pos = BlockPos::new(x, y, z);
                let block = ctx.plot.get_block(block_pos);
                redstone::update(block, ctx.plot, block_pos);
            }
        }
    }

    ctx.player.send_worldedit_message(&format!(
        "Your selection was updated sucessfully. ({:?})",
        start_time.elapsed()
    ));
}

pub(super) fn execute_replace_container(ctx: CommandExecuteContext<'_>) {
    let start_time = Instant::now();

    let from = ctx.arguments[0].unwrap_container_type();
    let to = ctx.arguments[1].unwrap_container_type();

    let new_block = match to {
        ContainerType::Furnace => Block::Furnace {},
        ContainerType::Barrel => Block::Barrel {},
        ContainerType::Hopper => Block::Hopper {},
    };
    let slots = to.num_slots() as u32;

    let operation = worldedit_start_operation(ctx.player);
    for x in operation.x_range() {
        for y in operation.y_range() {
            for z in operation.z_range() {
                let pos = BlockPos::new(x, y, z);
                let block = ctx.plot.get_block(pos);

                if !matches!(
                    block,
                    Block::Furnace {} | Block::Barrel {} | Block::Hopper {}
                ) {
                    continue;
                }
                let block_entity = ctx.plot.get_block_entity(pos);
                if let Some(BlockEntity::Container {
                    comparator_override,
                    ty,
                    ..
                }) = block_entity
                {
                    if *ty != from {
                        continue;
                    }
                    let ss = *comparator_override;

                    let items_needed = match ss {
                        0 => 0,
                        15 => slots * 64,
                        _ => ((32 * slots * ss as u32) as f32 / 7.0 - 1.0).ceil() as u32,
                    } as usize;
                    let mut inventory = Vec::new();
                    for (slot, items_added) in (0..items_needed).step_by(64).enumerate() {
                        let count = (items_needed - items_added).min(64);
                        inventory.push(InventoryEntry {
                            id: Item::Redstone {}.get_id(),
                            slot: slot as i8,
                            count: count as i8,
                            nbt: None,
                        });
                    }

                    let new_entity = BlockEntity::Container {
                        comparator_override: ss,
                        inventory,
                        ty: to,
                    };
                    ctx.plot.set_block_entity(pos, new_entity);
                    ctx.plot.set_block(pos, new_block);
                }
            }
        }
    }

    ctx.player.send_worldedit_message(&format!(
        "Your selection was replaced sucessfully. ({:?})",
        start_time.elapsed()
    ));
}

pub(super) fn execute_unimplemented(_ctx: CommandExecuteContext<'_>) {
    unimplemented!("Unimplimented worldedit command");
}
