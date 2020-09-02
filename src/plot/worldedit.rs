use super::Plot;
use crate::blocks::{Block, BlockEntity, BlockFacing, BlockPos};
use crate::network::packets::clientbound::*;
use crate::player::Player;
use crate::world::storage::PalettedBitBuffer;
use crate::world::World;
use rand::Rng;
use regex::Regex;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::ops::RangeInclusive;
use std::time::Instant;

// Attempts to execute a worldedit command. Returns true of the command was handled.
pub fn execute_command(
    plot: &mut Plot,
    player_idx: usize,
    command: &str,
    args: &mut Vec<&str>,
) -> bool {
    let command = if let Some(command) = COMMANDS.get(command) {
        command
    } else if let Some(command) = ALIASES.get(command) {
        let mut alias: Vec<&str> = command.split(' ').collect();
        let command = alias.remove(0);
        if alias.len() > 1 {
            args.append(&mut alias);
        }
        &COMMANDS[command]
    } else {
        return false;
    };

    let mut ctx = CommandExecuteContext {
        plot,
        player_idx,
        arguments: Vec::new(),
        flags: &[],
    };

    if command.requires_positions {
        let player = ctx.get_player_mut();
        if player.first_position.is_none() || player.second_position.is_none() {
            player.send_error_message("Make a region selection first.");
            return true;
        }
    }

    if command.requires_clipboard {
        let player = ctx.get_player_mut();
        if player.worldedit_clipboard.is_none() {
            player.send_error_message("Your clipboard is empty. Use //copy first.");
            return true;
        }
    }

    let flag_descs = command.flags;

    let mut arg_removal_idxs = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if arg.starts_with('-') {
            let mut with_argument = false;
            let flags = arg.chars();
            for flag in flags.skip(1) {
                if with_argument {
                    ctx.get_player_mut()
                        .send_error_message("Flag with argument must be last in grouping");
                    return true;
                }
                let flag_desc = if let Some(desc) = flag_descs.iter().find(|d| d.letter == flag) {
                    desc
                } else {
                    ctx.get_player_mut()
                        .send_error_message(&format!("Unknown flag: {}", flag));
                    return true;
                };
                arg_removal_idxs.push(i);
                if flag_desc.argument_type.is_some() {
                    arg_removal_idxs.push(i + 1);
                    with_argument = true;
                }
            }
        }
    }

    for idx in arg_removal_idxs.iter().rev() {
        args.remove(*idx);
    }

    let arg_descs = command.arguments;

    if args.len() > arg_descs.len() {
        ctx.get_player_mut()
            .send_error_message("Too many arguments.");
        return true;
    }

    for (i, arg_desc) in arg_descs.iter().enumerate() {
        let arg = args.get(i).map(|s| *s);
        match Argument::parse(&ctx, arg_desc.argument_type, arg) {
            Ok(default_arg) => ctx.arguments.push(default_arg),
            Err(err) => {
                ctx.get_player_mut().send_error_message(&err.to_string());
                return true;
            }
        }
    }

    (command.execute_fn)(ctx);
    true
}

#[derive(Debug)]
struct ArgumentParseError {
    arg_type: ArgumentType,
    reason: String,
}

impl ArgumentParseError {
    fn new(arg_type: ArgumentType, reason: &str) -> ArgumentParseError {
        ArgumentParseError {
            arg_type,
            reason: String::from(reason),
        }
    }
}

impl fmt::Display for ArgumentParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Error parsing argument of type {:?}: {}",
            self.arg_type, self.reason
        )
    }
}

impl std::error::Error for ArgumentParseError {}

type ArgumentParseResult = Result<Argument, ArgumentParseError>;

#[derive(Copy, Clone, Debug)]
enum ArgumentType {
    UnsignedInteger,
    Direction,
    Mask,
    Pattern,
}

enum Argument {
    UnsignedInteger(u32),
    Direction(BlockFacing),
    Pattern(WorldEditPattern),
    Mask(WorldEditPattern),
}

impl Argument {
    fn unwrap_uint(&self) -> u32 {
        match self {
            Argument::UnsignedInteger(val) => *val,
            _ => panic!("Argument was not an UnsignedInteger"),
        }
    }

    fn unwrap_direction(&self) -> &BlockFacing {
        match self {
            Argument::Direction(val) => val,
            _ => panic!("Argument was not an UnsignedInteger"),
        }
    }

    fn unwrap_pattern(&self) -> &WorldEditPattern {
        match self {
            Argument::Pattern(val) => val,
            _ => panic!("Argument was not an UnsignedInteger"),
        }
    }

    fn get_default(ctx: &CommandExecuteContext, arg_type: ArgumentType) -> ArgumentParseResult {
        match arg_type {
            ArgumentType::Direction => Argument::parse(ctx, arg_type, Some("me")),
            ArgumentType::UnsignedInteger => Ok(Argument::UnsignedInteger(1)),
            _ => Err(ArgumentParseError::new(
                arg_type,
                "argument can't be inferred",
            )),
        }
    }

    fn parse(
        ctx: &CommandExecuteContext,
        arg_type: ArgumentType,
        arg: Option<&str>,
    ) -> ArgumentParseResult {
        if arg.is_none() {
            return Ok(Argument::get_default(ctx, arg_type)?);
        }
        let arg = arg.unwrap();
        match arg_type {
            ArgumentType::Direction => {
                let player_facing = ctx.get_player().get_facing();
                match arg {
                    "me" => Ok(Argument::Direction(player_facing)),
                    _ => Err(ArgumentParseError::new(arg_type, "unknown direction")),
                }
            }
            ArgumentType::UnsignedInteger => match arg.parse::<u32>() {
                Ok(num) => Ok(Argument::UnsignedInteger(num)),
                Err(_) => Err(ArgumentParseError::new(arg_type, "error parsing uint")),
            },
            ArgumentType::Pattern => match WorldEditPattern::from_str(arg) {
                Ok(pattern) => Ok(Argument::Pattern(pattern)),
                Err(err) => Err(ArgumentParseError::new(arg_type, &err.to_string())),
            },
            // Masks are net yet implemented, so in the meantime they can be treated as patterns
            ArgumentType::Mask => match WorldEditPattern::from_str(arg) {
                Ok(pattern) => Ok(Argument::Pattern(pattern)),
                Err(err) => Err(ArgumentParseError::new(arg_type, &err.to_string())),
            },
        }
    }
}

struct ArgumentDescription {
    name: &'static str,
    argument_type: ArgumentType,
    description: &'static str,
}

macro_rules! argument {
    ($name:literal, $type:ident, $desc:literal) => {
        ArgumentDescription {
            name: $name,
            argument_type: ArgumentType::$type,
            description: $desc,
        }
    };
}

struct FlagDescription {
    letter: char,
    argument_type: Option<ArgumentType>,
    description: &'static str,
}

struct CommandExecuteContext<'a> {
    plot: &'a mut Plot,
    player_idx: usize,
    arguments: Vec<Argument>,
    flags: &'a [char],
}

impl<'a> CommandExecuteContext<'a> {
    fn has_flag(&self, c: char) -> bool {
        self.flags.contains(&c)
    }

    fn get_player(&self) -> &Player {
        &self.plot.players[self.player_idx]
    }

    fn get_player_mut(&mut self) -> &mut Player {
        &mut self.plot.players[self.player_idx]
    }
}

struct WorldeditCommand {
    arguments: &'static [ArgumentDescription],
    flags: &'static [FlagDescription],
    requires_positions: bool,
    requires_clipboard: bool,
    execute_fn: fn(CommandExecuteContext),
    description: &'static str,
}

impl Default for WorldeditCommand {
    fn default() -> Self {
        Self {
            arguments: &[],
            flags: &[],
            execute_fn: execute_unimplemented,
            description: "",
            requires_clipboard: false,
            requires_positions: false,
        }
    }
}

macro_rules! map(
    { $($key:expr => $value:expr),+ } => {
        {
            let mut m = ::std::collections::HashMap::new();
            $(
                m.insert($key, $value);
            )+
            m
        }
     };
);

lazy_static! {
    static ref COMMANDS: HashMap<&'static str, WorldeditCommand> = map! {
        "copy" => WorldeditCommand {
            requires_positions: true,
            execute_fn: execute_copy,
            description: "Copy the selection to the clipboard",
            ..Default::default()
        },
        "paste" => WorldeditCommand {
            requires_clipboard: true,
            execute_fn: execute_paste,
            description: "Paste the clipboard's contents",
            ..Default::default()
        },
        "undo" => WorldeditCommand {
            execute_fn: execute_undo,
            description: "Undo's the last action (from history)",
            ..Default::default()
        },
        "stack" => WorldeditCommand {
            arguments: &[
                argument!("count", UnsignedInteger, "# of copies to stack"),
                argument!("direction", Direction, "The direction to stack")
            ],
            requires_positions: true,
            execute_fn: execute_stack,
            description: "Repeat the contents of the selection",
            ..Default::default()
        },
        "count" => WorldeditCommand {
            arguments: &[
                argument!("mask", Mask, "The mask of blocks to match")
            ],
            requires_positions: true,
            execute_fn: execute_count,
            description: "Counts the number of blocks matching a mask",
            ..Default::default()
        },
        "sel" => WorldeditCommand {
            execute_fn: execute_sel,
            description: "Choose a region selector",
            ..Default::default()
        },
        "set" => WorldeditCommand {
            arguments: &[
                argument!("pattern", Pattern, "The pattern of blocks to set")
            ],
            requires_positions: true,
            execute_fn: execute_set,
            description: "Sets all the blocks in the region",
            ..Default::default()
        },
        "pos1" => WorldeditCommand {
            execute_fn: execute_pos1,
            description: "Set position 1",
            ..Default::default()
        },
        "pos2" => WorldeditCommand {
            execute_fn: execute_pos2,
            description: "Set position 2",
            ..Default::default()
        },
        "replace" => WorldeditCommand {
            arguments: &[
                argument!("from", Mask, "The mask representng blocks to replace"),
                argument!("to", Pattern, "The pattern of blocks to replace with")
            ],
            requires_positions: true,
            execute_fn: execute_replace,
            description: "Replace all blocks in a selection with another",
            ..Default::default()
        }
    };
}

lazy_static! {
    static ref ALIASES: HashMap<&'static str, &'static str> = map! {
        "1" => "pos1",
        "2" => "pos2",
        "c" => "copy",
        "x" => "cut",
        "v" => "paste",
        "va" => "paste -a",
        "s" => "stack",
        "sa" => "stack -a"
    };
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

#[derive(Clone, Debug)]
pub struct WorldEditUndo {
    clipboard: WorldEditClipboard,
    pos: BlockPos,
    plot_x: i32,
    plot_z: i32,
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
                y: pos_array[1],
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

impl fmt::Display for PatternParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PatternParseError::UnknownBlock(block) => write!(f, "unknown block: {}", block),
            PatternParseError::InvalidPattern(pattern) => write!(f, "invalid pattern: {}", pattern),
        }
    }
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
    pub records: Vec<C0FMultiBlockChange>,
    x_range: RangeInclusive<i32>,
    y_range: RangeInclusive<i32>,
    z_range: RangeInclusive<i32>,
}

impl WorldEditOperation {
    fn new(first_pos: BlockPos, second_pos: BlockPos) -> WorldEditOperation {
        let start_pos = first_pos.min(second_pos);
        let end_pos = first_pos.max(second_pos);

        let mut records: Vec<C0FMultiBlockChange> = Vec::new();

        for chunk_x in (start_pos.x >> 4)..=(end_pos.x >> 4) {
            for chunk_z in (start_pos.z >> 4)..=(end_pos.z >> 4) {
                records.push(C0FMultiBlockChange {
                    chunk_x,
                    chunk_z,
                    records: Vec::new(),
                });
            }
        }

        let x_range = start_pos.x..=end_pos.x;
        let y_range = start_pos.y..=end_pos.y;
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
            packet.records.push(C0FMultiBlockChangeRecord {
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
    fn y_range(&self) -> RangeInclusive<i32> {
        self.y_range.to_owned()
    }
    fn z_range(&self) -> RangeInclusive<i32> {
        self.z_range.to_owned()
    }
}

fn worldedit_send_operation(plot: &mut Plot, operation: WorldEditOperation) {
    for packet in operation.records {
        let chunk = match plot.get_chunk(packet.chunk_x, packet.chunk_z) {
            Some(chunk) => chunk,
            None => continue,
        };
        let chunk_data = chunk.encode_packet(false);
        for player in &mut plot.players {
            player.client.send_packet(&chunk_data);
        }
    }
}

fn worldedit_start_operation(plot: &mut Plot, player: usize) -> Option<WorldEditOperation> {
    let player = &mut plot.players[player];
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
    if !Plot::in_plot_bounds(plot.x, plot.z, first_pos.x, first_pos.z) {
        player.send_system_message("First position is outside plot bounds!");
        return None;
    }
    if !Plot::in_plot_bounds(plot.x, plot.z, first_pos.x, first_pos.z) {
        player.send_system_message("Second position is outside plot bounds!");
        return None;
    }

    Some(WorldEditOperation::new(first_pos, second_pos))
}

fn execute_set(mut ctx: CommandExecuteContext) {
    let start_time = Instant::now();
    let pattern = ctx.arguments[0].unwrap_pattern();

    if let Some(mut operation) = worldedit_start_operation(ctx.plot, ctx.player_idx) {
        capture_undo(
            ctx.plot,
            ctx.player_idx,
            ctx.get_player().first_position.unwrap(),
            ctx.get_player().second_position.unwrap(),
        );
        for x in operation.x_range() {
            for y in operation.y_range() {
                for z in operation.z_range() {
                    let block_pos = BlockPos::new(x, y, z);
                    let block_id = pattern.pick().get_id();

                    if ctx.plot.set_block_raw(block_pos, block_id) {
                        operation.update_block(block_pos, block_id);
                    }
                }
            }
        }

        let blocks_updated = operation.blocks_updated();
        worldedit_send_operation(ctx.plot, operation);

        ctx.get_player_mut().send_worldedit_message(&format!(
            "Operation completed: {} block(s) affected ({:?})",
            blocks_updated,
            start_time.elapsed()
        ));
    }
}

fn execute_replace(mut ctx: CommandExecuteContext) {
    let start_time = Instant::now();

    let filter = ctx.arguments[0].unwrap_pattern();
    let pattern = ctx.arguments[1].unwrap_pattern();

    if let Some(mut operation) = worldedit_start_operation(ctx.plot, ctx.player_idx) {
        capture_undo(
            ctx.plot,
            ctx.player_idx,
            ctx.get_player().first_position.unwrap(),
            ctx.get_player().second_position.unwrap(),
        );
        for x in operation.x_range() {
            for y in operation.y_range() {
                for z in operation.z_range() {
                    let block_pos = BlockPos::new(x, y, z);

                    if filter.matches(ctx.plot.get_block(block_pos)) {
                        let block_id = pattern.pick().get_id();

                        if ctx.plot.set_block_raw(block_pos, block_id) {
                            operation.update_block(block_pos, block_id);
                        }
                    }
                }
            }
        }

        let blocks_updated = operation.blocks_updated();
        worldedit_send_operation(ctx.plot, operation);

        ctx.get_player_mut().send_worldedit_message(&format!(
            "Operation completed: {} block(s) affected ({:?})",
            blocks_updated,
            start_time.elapsed()
        ));
    }
}

fn execute_count(mut ctx: CommandExecuteContext) {
    let start_time = Instant::now();

    let filter = ctx.arguments[0].unwrap_pattern();

    if let Some(operation) = worldedit_start_operation(ctx.plot, ctx.player_idx) {
        let mut blocks_counted = 0;

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

        ctx.get_player_mut().send_worldedit_message(&format!(
            "Counted {} block(s) ({:?})",
            blocks_counted,
            start_time.elapsed()
        ));
    }
}

fn create_clipboard(
    plot: &mut Plot,
    origin: BlockPos,
    first_pos: BlockPos,
    second_pos: BlockPos,
) -> WorldEditClipboard {
    let start_pos = first_pos.min(second_pos);
    let end_pos = first_pos.max(second_pos);
    let size_x = (end_pos.x - start_pos.x) as u32 + 1;
    let size_y = (end_pos.y - start_pos.y) as u32 + 1;
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
                    .set_entry(i, plot.get_block_raw(BlockPos::new(x, y, z)));
                i += 1;
            }
        }
    }
    cb
}

fn paste_clipboard(plot: &mut Plot, cb: &WorldEditClipboard, pos: BlockPos) {
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
                plot.set_block_raw(BlockPos::new(x, y, z), cb.data.get_entry(i));
                i += 1;
            }
        }
    }
    let chunk_x_range =
        (offset_x - (plot.x << 8)) >> 4..=(offset_x + cb.size_x as i32 - (plot.x << 8)) >> 4;
    let chunk_z_range =
        (offset_z - (plot.z << 8)) >> 4..=(offset_z + cb.size_z as i32 - (plot.z << 8)) >> 4;
    for chunk_x in chunk_x_range {
        for chunk_z in chunk_z_range.clone() {
            if let Some(chunk) = plot.get_chunk(chunk_x, chunk_z) {
                let chunk_data = chunk.encode_packet(false);
                for player in &mut plot.players {
                    player.client.send_packet(&chunk_data);
                }
            }
        }
    }
    for (pos, block_entity) in &cb.block_entities {
        let new_pos = BlockPos {
            x: pos.x + offset_x,
            y: pos.y + offset_y,
            z: pos.z + offset_z,
        };
        plot.set_block_entity(new_pos, block_entity.clone());
    }
}

fn capture_undo(plot: &mut Plot, player: usize, first_pos: BlockPos, second_pos: BlockPos) {
    let origin = first_pos.min(second_pos);
    let cb = create_clipboard(plot, origin, first_pos, second_pos);
    let undo = WorldEditUndo {
        clipboard: cb,
        pos: origin,
        plot_x: plot.x,
        plot_z: plot.z,
    };
    plot.players[player].worldedit_undo.push(undo);
}

fn execute_copy(mut ctx: CommandExecuteContext) {
    let start_time = Instant::now();

    let origin = BlockPos::new(
        ctx.get_player().x.floor() as i32,
        ctx.get_player().y.floor() as i32,
        ctx.get_player().z.floor() as i32,
    );
    let clipboard = create_clipboard(
        ctx.plot,
        origin,
        ctx.get_player().first_position.unwrap(),
        ctx.get_player().second_position.unwrap(),
    );
    ctx.get_player_mut().worldedit_clipboard = Some(clipboard);

    ctx.get_player_mut().send_worldedit_message(&format!(
        "Your selection was copied. ({:?})",
        start_time.elapsed()
    ));
}

fn execute_paste(mut ctx: CommandExecuteContext) {
    let start_time = Instant::now();

    if ctx.get_player().worldedit_clipboard.is_some() {
        // Here I am cloning the clipboard. This is bad. Don't do this.
        let cb = &ctx.get_player().worldedit_clipboard.clone().unwrap();
        let pos = BlockPos::new(
            ctx.get_player().x.floor() as i32,
            ctx.get_player().y.floor() as i32,
            ctx.get_player().z.floor() as i32,
        );
        let offset_x = pos.x - cb.offset_x;
        let offset_y = pos.y - cb.offset_y;
        let offset_z = pos.z - cb.offset_z;
        capture_undo(
            ctx.plot,
            ctx.player_idx,
            BlockPos::new(offset_x, offset_y, offset_z),
            BlockPos::new(
                offset_x + cb.size_x as i32,
                offset_y + cb.size_y as i32,
                offset_z + cb.size_z as i32,
            ),
        );
        paste_clipboard(ctx.plot, cb, pos);
        ctx.get_player_mut().send_worldedit_message(&format!(
            "Your clipboard was pasted. ({:?})",
            start_time.elapsed()
        ));
    } else {
        ctx.get_player_mut()
            .send_system_message("Your clipboard is empty!");
    }
}

// TODO: This should use the new worldedit command stuff
pub(super) fn execute_load(plot: &mut Plot, player: usize, file_name: &str) {
    let start_time = Instant::now();

    let clipboard = WorldEditClipboard::load_from_schematic(file_name);
    match clipboard {
        Some(cb) => {
            plot.players[player].worldedit_clipboard = Some(cb);
            plot.players[player].send_worldedit_message(&format!(
                "The schematic was loaded to your clipboard. Do //paste to birth it into the world. ({:?})",
                start_time.elapsed()
            ));
        }
        None => {
            plot.players[player].send_error_message("There was an error loading the schematic.");
        }
    }
}

fn execute_find(plot: &mut Plot, player: usize, block_id: u32) {
    let start_time = Instant::now();

    if let Some(operation) = worldedit_start_operation(plot, player) {
        for x in operation.x_range() {
            for y in operation.y_range() {
                for z in operation.z_range() {
                    let block_pos = BlockPos::new(x, y, z);
                    if plot.get_block_raw(block_pos) == block_id {
                        plot.players[player].send_worldedit_message(&format!(
                            "The block was found at {:?}",
                            block_pos
                        ));
                    }
                }
            }
        }
        plot.players[player].send_worldedit_message(&format!("Done. ({:?})", start_time.elapsed()));
    }
}

fn execute_stack(mut ctx: CommandExecuteContext) {
    let start_time = Instant::now();

    let stack_amt = ctx.arguments[0].unwrap_uint();
    let direction = ctx.arguments[1].unwrap_direction();
    let pos1 = ctx.get_player().first_position.unwrap();
    let clipboard = create_clipboard(
        ctx.plot,
        pos1,
        pos1,
        ctx.get_player().second_position.unwrap(),
    );
    let mut all_pos: Vec<BlockPos> = Vec::new();
    let stack_offset = match direction {
        BlockFacing::North | BlockFacing::South => clipboard.size_z,
        BlockFacing::East | BlockFacing::West => clipboard.size_x,
        BlockFacing::Up | BlockFacing::Down => clipboard.size_y,
    };
    for i in 1..stack_amt + 1 {
        all_pos.push(direction.offset_pos(pos1, (i * stack_offset) as i32));
    }
    for block_pos in all_pos {
        paste_clipboard(ctx.plot, &clipboard, block_pos);
    }
    ctx.get_player_mut().send_worldedit_message(&format!(
        "Your clipboard was stacked. ({:?})",
        start_time.elapsed()
    ));
}

fn execute_undo(mut ctx: CommandExecuteContext) {
    if ctx.get_player().worldedit_undo.is_empty() {
        ctx.get_player_mut()
            .send_error_message("There is nothing left to undo.");
        return;
    }
    let undo = ctx.get_player_mut().worldedit_undo.pop().unwrap();
    if undo.plot_x != ctx.plot.x || undo.plot_z != ctx.plot.z {
        ctx.get_player_mut()
            .send_error_message("Cannot undo outside of your current plot.");
        return;
    }
    paste_clipboard(ctx.plot, &undo.clipboard, undo.pos);
}

fn execute_sel(mut ctx: CommandExecuteContext) {
    let player = ctx.get_player_mut();
    player.first_position = None;
    player.second_position = None;
    player.send_worldedit_message("Selection cleared.");
}

fn execute_pos1(mut ctx: CommandExecuteContext) {
    let player = ctx.get_player_mut();

    let x = player.x as i32;
    let y = player.y as i32;
    let z = player.z as i32;

    player.worldedit_set_first_position(x, y, z);
}

fn execute_pos2(mut ctx: CommandExecuteContext) {
    let player = ctx.get_player_mut();

    let x = player.x as i32;
    let y = player.y as i32;
    let z = player.z as i32;

    player.worldedit_set_second_position(x, y, z);
}

fn execute_unimplemented(ctx: CommandExecuteContext) {
    unimplemented!("Unimplimented worldedit command");
}
