//! [Worldedit](https://github.com/EngineHub/WorldEdit) and [RedstoneTools](https://github.com/paulikauro/RedstoneTools) implementation

mod schematic;

use super::{Plot, PlotWorld};
use crate::blocks::{Block, BlockEntity, BlockFace, BlockFacing, BlockPos, FlipDirection, RotateAmt};
use crate::chat::{ChatComponentBuilder, ColorCode};
use crate::player::{Player, PlayerPos};
use crate::world::storage::PalettedBitBuffer;
use crate::world::World;
use log::error;
use rand::Rng;
use regex::Regex;
use schematic::{load_schematic, save_schematic};
use std::collections::HashMap;
use std::fmt;
use std::lazy::SyncLazy;
use std::ops::RangeInclusive;
use std::time::Instant;

// Attempts to execute a worldedit command. Returns true of the command was handled.
// The command is not handled if it is not found in the worldedit commands and alias lists.
pub fn execute_command(
    plot: &mut Plot,
    player_idx: usize,
    command: &str,
    args: &mut Vec<&str>,
) -> bool {
    let player = &mut plot.players[player_idx];
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
        plot: &mut plot.world,
        player,
        arguments: Vec::new(),
        flags: Vec::new(),
    };

    let wea = ctx.player.has_permission("plots.worldedit.bypass");
    if !wea {
        if let Some(owner) = plot.owner {
            if owner != ctx.player.uuid {
                // tried to worldedit on plot that wasn't theirs
                ctx.player.send_no_permission_message();
                return true;
            }
        } else {
            // tried to worldedit on unclaimed plot
            ctx.player.send_no_permission_message();
            return true;
        }
    }

    if !command.permission_node.is_empty() && !ctx.player.has_permission(command.permission_node) {
        ctx.player.send_no_permission_message();
        return true;
    }

    if command.requires_positions {
        let plot_x = ctx.plot.x;
        let plot_z = ctx.plot.z;
        let player = &mut ctx.player;
        if player.first_position.is_none() || player.second_position.is_none() {
            player.send_error_message("Make a region selection first.");
            return true;
        }
        let first_pos = player.first_position.unwrap();
        let second_pos = player.second_position.unwrap();
        if !Plot::in_plot_bounds(plot_x, plot_z, first_pos.x, first_pos.z) {
            player.send_system_message("First position is outside plot bounds!");
            return true;
        }
        if !Plot::in_plot_bounds(plot_x, plot_z, second_pos.x, second_pos.z) {
            player.send_system_message("Second position is outside plot bounds!");
            return true;
        }
    }

    if command.requires_clipboard {
        let player = &mut ctx.player;
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
                    ctx.player
                        .send_error_message("Flag with argument must be last in grouping");
                    return true;
                }
                let flag_desc = if let Some(desc) = flag_descs.iter().find(|d| d.letter == flag) {
                    desc
                } else {
                    ctx.player
                        .send_error_message(&format!("Unknown flag: {}", flag));
                    return true;
                };
                arg_removal_idxs.push(i);
                if flag_desc.argument_type.is_some() {
                    arg_removal_idxs.push(i + 1);
                    with_argument = true;
                }
                ctx.flags.push(flag);
            }
        }
    }

    for idx in arg_removal_idxs.iter().rev() {
        args.remove(*idx);
    }

    let arg_descs = command.arguments;

    if args.len() > arg_descs.len() {
        ctx.player.send_error_message("Too many arguments.");
        return true;
    }

    for (i, arg_desc) in arg_descs.iter().enumerate() {
        let arg = args.get(i).copied();
        match Argument::parse(&ctx, arg_desc, arg) {
            Ok(default_arg) => ctx.arguments.push(default_arg),
            Err(err) => {
                ctx.player.send_error_message(&err.to_string());
                return true;
            }
        }
    }
    plot.redpiler.reset(ctx.plot);
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
    /// Used for diag directions in redstonetools commands
    DirectionVector,
    Mask,
    Pattern,
    String,
}

#[derive(Debug, Clone)]
enum Argument {
    UnsignedInteger(u32),
    Direction(BlockFacing),
    DirectionVector(BlockPos),
    Pattern(WorldEditPattern),
    Mask(WorldEditPattern),
    String(String),
}

impl Argument {
    fn unwrap_uint(&self) -> u32 {
        match self {
            Argument::UnsignedInteger(val) => *val,
            _ => panic!("Argument was not an UnsignedInteger"),
        }
    }

    fn unwrap_direction(&self) -> BlockFacing {
        match self {
            Argument::Direction(val) => *val,
            _ => panic!("Argument was not an Direction"),
        }
    }

    fn unwrap_direction_vec(&self) -> BlockPos {
        match self {
            Argument::DirectionVector(val) => *val,
            _ => panic!("Argument was not an DirectionVector"),
        }
    }

    fn unwrap_pattern(&self) -> &WorldEditPattern {
        match self {
            Argument::Pattern(val) => val,
            _ => panic!("Argument was not a Pattern"),
        }
    }

    fn unwrap_mask(&self) -> &WorldEditPattern {
        match self {
            Argument::Mask(val) => val,
            _ => panic!("Argument was not a Mask"),
        }
    }

    fn unwrap_string(&self) -> &String {
        match self {
            Argument::String(val) => val,
            _ => panic!("Argument was not a String"),
        }
    }

    fn get_default(
        ctx: &CommandExecuteContext<'_>,
        desc: &ArgumentDescription,
    ) -> ArgumentParseResult {
        if let Some(default) = &desc.default {
            return Ok(default.clone());
        }

        let arg_type = desc.argument_type;
        match arg_type {
            ArgumentType::Direction | ArgumentType::DirectionVector => {
                Argument::parse(ctx, desc, Some("me"))
            }
            ArgumentType::UnsignedInteger => Ok(Argument::UnsignedInteger(1)),
            _ => Err(ArgumentParseError::new(
                arg_type,
                "argument can't be inferred",
            )),
        }
    }

    fn parse(
        ctx: &CommandExecuteContext<'_>,
        desc: &ArgumentDescription,
        arg: Option<&str>,
    ) -> ArgumentParseResult {
        if arg.is_none() {
            return Argument::get_default(ctx, desc);
        }
        let arg = arg.unwrap();
        let arg_type = desc.argument_type;
        match arg_type {
            ArgumentType::Direction => {
                let player_facing = ctx.player.get_facing();
                Ok(Argument::Direction(match arg {
                    "me" => player_facing,
                    "u" | "up" => BlockFacing::Up,
                    "d" | "down" => BlockFacing::Down,
                    "l" | "left" => player_facing.rotate_ccw(),
                    "r" | "right" => player_facing.rotate(),
                    _ => return Err(ArgumentParseError::new(arg_type, "unknown direction")),
                }))
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
                Ok(pattern) => Ok(Argument::Mask(pattern)),
                Err(err) => Err(ArgumentParseError::new(arg_type, &err.to_string())),
            },
            ArgumentType::String => Ok(Argument::String(arg.to_owned())),
            ArgumentType::DirectionVector => {
                let mut vec = BlockPos::new(0, 0, 0);
                let player_facing = ctx.player.get_facing();
                if arg == "me" {
                    vec = player_facing.offset_pos(vec, 1);
                    if !matches!(player_facing, BlockFacing::Down | BlockFacing::Up) {
                        let pitch = ctx.player.pitch;
                        if pitch > 22.5 {
                            vec.y -= 1;
                        } else if pitch < -22.5 {
                            vec.y += 1;
                        }
                    }
                    return Ok(Argument::DirectionVector(vec));
                }

                let mut base_dir = arg;
                if arg.len() > 1 && matches!(arg.chars().last(), Some('u' | 'd')) {
                    match arg.chars().last().unwrap() {
                        'u' => vec.y += 1,
                        'd' => vec.y -= 1,
                        _ => unreachable!(),
                    }
                    base_dir = &arg[..1];
                }

                let facing = match base_dir {
                    "u" | "up" => BlockFacing::Up,
                    "d" | "down" => BlockFacing::Down,
                    "l" | "left" => player_facing.rotate_ccw(),
                    "r" | "right" => player_facing.rotate(),
                    _ => return Err(ArgumentParseError::new(arg_type, "unknown direction")),
                };
                let vec = facing.offset_pos(vec, 1);

                Ok(Argument::DirectionVector(vec))
            }
        }
    }
}

struct ArgumentDescription {
    name: &'static str,
    argument_type: ArgumentType,
    description: &'static str,
    default: Option<Argument>,
}

macro_rules! argument {
    ($name:literal, $type:ident, $desc:literal) => {
        ArgumentDescription {
            name: $name,
            argument_type: ArgumentType::$type,
            description: $desc,
            default: None,
        }
    };
    ($name:literal, $type:ident, $desc:literal, $default:literal) => {
        ArgumentDescription {
            name: $name,
            argument_type: ArgumentType::$type,
            description: $desc,
            default: Some(Argument::$type($default)),
        }
    };
}

struct FlagDescription {
    letter: char,
    argument_type: Option<ArgumentType>,
    description: &'static str,
}

macro_rules! flag {
    ($name:literal, $type:ident, $desc:literal) => {
        FlagDescription {
            letter: $name,
            argument_type: $type,
            description: $desc,
        }
    };
}

struct CommandExecuteContext<'a> {
    plot: &'a mut PlotWorld,
    player: &'a mut Player,
    arguments: Vec<Argument>,
    flags: Vec<char>,
}

impl<'a> CommandExecuteContext<'a> {
    fn has_flag(&self, c: char) -> bool {
        self.flags.contains(&c)
    }
}

struct WorldeditCommand {
    arguments: &'static [ArgumentDescription],
    flags: &'static [FlagDescription],
    requires_positions: bool,
    requires_clipboard: bool,
    execute_fn: fn(CommandExecuteContext<'_>),
    description: &'static str,
    permission_node: &'static str,
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
            permission_node: "",
        }
    }
}

static COMMANDS: SyncLazy<HashMap<&'static str, WorldeditCommand>> = SyncLazy::new(|| {
    map! {
        "up" => WorldeditCommand {
            execute_fn: execute_up,
            description: "Go upwards some distance",
            arguments: &[
                argument!("distance", UnsignedInteger, "Distance to go upwards")
            ],
            permission_node: "worldedit.navigation.up",
            ..Default::default()
        },
        "/pos1" => WorldeditCommand {
            execute_fn: execute_pos1,
            description: "Set position 1",
            permission_node: "worldedit.selection.pos",
            ..Default::default()
        },
        "/pos2" => WorldeditCommand {
            execute_fn: execute_pos2,
            description: "Set position 2",
            permission_node: "worldedit.selection.pos",
            ..Default::default()
        },
        "/hpos1" => WorldeditCommand {
            execute_fn: execute_hpos1,
            description: "Set position 1 to targeted block",
            permission_node: "worldedit.selection.hpos",
            ..Default::default()
        },
        "/hpos2" => WorldeditCommand {
            execute_fn: execute_hpos2,
            description: "Set position 2 to targeted block",
            permission_node: "worldedit.selection.hpos",
            ..Default::default()
        },
        "/sel" => WorldeditCommand {
            execute_fn: execute_sel,
            description: "Choose a region selector",
            ..Default::default()
        },
        "/set" => WorldeditCommand {
            arguments: &[
                argument!("pattern", Pattern, "The pattern of blocks to set")
            ],
            requires_positions: true,
            execute_fn: execute_set,
            description: "Sets all the blocks in the region",
            permission_node: "worldedit.region.stack",
            ..Default::default()
        },
        "/replace" => WorldeditCommand {
            arguments: &[
                argument!("from", Mask, "The mask representng blocks to replace"),
                argument!("to", Pattern, "The pattern of blocks to replace with")
            ],
            requires_positions: true,
            execute_fn: execute_replace,
            description: "Replace all blocks in a selection with another",
            permission_node: "worldedit.region.replace",
            ..Default::default()
        },
        "/copy" => WorldeditCommand {
            requires_positions: true,
            execute_fn: execute_copy,
            description: "Copy the selection to the clipboard",
            permission_node: "worldedit.clipboard.copy",
            ..Default::default()
        },
        "/cut" => WorldeditCommand {
            requires_positions: true,
            execute_fn: execute_cut,
            description: "Cut the selection to the clipboard",
            permission_node: "worldedit.clipboard.cut",
            ..Default::default()
        },
        "/paste" => WorldeditCommand {
            requires_clipboard: true,
            execute_fn: execute_paste,
            description: "Paste the clipboard's contents",
            flags: &[
                flag!('a', None, "Skip air blocks")
            ],
            permission_node: "worldedit.clipboard.paste",
            ..Default::default()
        },
        "/undo" => WorldeditCommand {
            execute_fn: execute_undo,
            description: "Undoes the last action (from history)",
            permission_node: "worldedit.history.undo",
            ..Default::default()
        },
        "/redo" => WorldeditCommand {
            execute_fn: execute_redo,
            description: "Redoes the last action (from history)",
            permission_node: "worldedit.history.redo",
            ..Default::default()
        },
        "/stack" => WorldeditCommand {
            arguments: &[
                argument!("count", UnsignedInteger, "# of copies to stack"),
                argument!("direction", Direction, "The direction to stack")
            ],
            requires_positions: true,
            execute_fn: execute_stack,
            description: "Repeat the contents of the selection",
            flags: &[
                flag!('a', None, "Ignore air blocks")
            ],
            permission_node: "worldedit.region.stack",
            ..Default::default()
        },
        "/move" => WorldeditCommand {
            arguments: &[
                argument!("count", UnsignedInteger, "The distance to move"),
                argument!("direction", Direction, "The direction to move")
            ],
            requires_positions: true,
            execute_fn: execute_move,
            description: "Move the contents of the selection",
            flags: &[
                flag!('a', None, "Ignore air blocks"),
                flag!('s', None, "Shift the selection to the target location")
            ],
            permission_node: "worldedit.region.move",
            ..Default::default()
        },
        "/count" => WorldeditCommand {
            arguments: &[
                argument!("mask", Mask, "The mask of blocks to match")
            ],
            requires_positions: true,
            execute_fn: execute_count,
            description: "Counts the number of blocks matching a mask",
            permission_node: "worldedit.analysis.count",
            ..Default::default()
        },
        "/load" => WorldeditCommand {
            arguments: &[
                argument!("name", String, "The file name of the schematic to load")
            ],
            execute_fn: execute_load,
            description: "Loads a schematic file into the clipboard",
            permission_node: "worldedit.clipboard.load",
            ..Default::default()
        },
        "/save" => WorldeditCommand {
            arguments: &[
                argument!("name", String, "The file name of the schematic to save")
            ],
            requires_clipboard: true,
            execute_fn: execute_save,
            description: "Save a schematic file from the clipboard",
            permission_node: "worldedit.clipboard.save",
            ..Default::default()
        },
        "/expand" => WorldeditCommand {
            arguments: &[
                argument!("amount", UnsignedInteger, "Amount to expand the selection by"),
                argument!("direction", Direction, "Direction to expand")
            ],
            requires_positions: true,
            execute_fn: execute_expand,
            description: "Expand the selection area",
            permission_node: "worldedit.selection.expand",
            ..Default::default()
        },
        "/contract" => WorldeditCommand {
            arguments: &[
                argument!("amount", UnsignedInteger, "Amount to contract the selection by"),
                argument!("direction", Direction, "Direction to contract")
            ],
            requires_positions: true,
            execute_fn: execute_contract,
            description: "Contract the selection area",
            permission_node: "worldedit.selection.contract",
            ..Default::default()
        },
        "/shift" => WorldeditCommand {
            arguments: &[
                argument!("amount", UnsignedInteger, "Amount to shift the selection by"),
                argument!("direction", Direction, "Direction to shift")
            ],
            requires_positions: true,
            execute_fn: execute_shift,
            description: "Shift the selection area",
            permission_node: "worldedit.selection.shift",
            ..Default::default()
        },
        "/flip" => WorldeditCommand {
            arguments: &[
                argument!("direction", Direction, "The direction to flip, defaults to look direction"),
            ],
            requires_clipboard: true,
            execute_fn: execute_flip,
            description: "Flip the contents of the clipboard across the origin",
            ..Default::default()
        },
        "/rotate" => WorldeditCommand {
            arguments: &[
                argument!("rotateY", UnsignedInteger, "Amount to rotate on the x-axis", 0),
            ],
            requires_clipboard: true,
            execute_fn: execute_rotate,
            description: "Rotate the contents of the clipboard",
            ..Default::default()
        },
        "/rstack" => WorldeditCommand {
            arguments: &[
                argument!("count", UnsignedInteger, "# of copies to stack"),
                argument!("spacing", UnsignedInteger, "The spacing between each selection", 2),
                argument!("direction", DirectionVector, "The direction to stack")
            ],
            flags: &[
                flag!('a', None, "Include air blocks"),
                flag!('e', None, "Expand selection")
            ],
            execute_fn: execute_rstack,
            description: "Like //stack but allows the stacked copies to overlap, supports more directions, and more flags",
            permission_node: "redstonetools.rstack",
            ..Default::default()
        },
        "/help" => WorldeditCommand {
            arguments: &[
                argument!("command", String, "Command to retrieve help for"),
            ],
            execute_fn: execute_help,
            description: "Displays help for WorldEdit commands",
            permission_node: "worldedit.help",
            ..Default::default()
        }
    }
});

static ALIASES: SyncLazy<HashMap<&'static str, &'static str>> = SyncLazy::new(|| {
    map! {
        "u" => "up",
        "/1" => "/pos1",
        "/2" => "/pos2",
        "/c" => "/copy",
        "/x" => "/cut",
        "/v" => "/paste",
        "/va" => "/paste -a",
        "/s" => "/stack",
        "/sa" => "/stack -a",
        "/e" => "/expand",
        "/r" => "/rotate",
        "/f" => "/flip",
        "/h1" => "/hpos1",
        "/h2" => "/hpos2",
        "/rs" => "/rstack"
    }
});

#[derive(Debug, Clone)]
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
    clipboards: Vec<WorldEditClipboard>,
    pos: BlockPos,
    plot_x: i32,
    plot_z: i32,
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

#[derive(Debug, Clone)]
pub struct WorldEditPattern {
    pub parts: Vec<WorldEditPatternPart>,
}

impl WorldEditPattern {
    pub fn from_str(pattern_str: &str) -> PatternParseResult<WorldEditPattern> {
        let mut pattern = WorldEditPattern { parts: Vec::new() };
        for part in pattern_str.split(',') {
            static RE: SyncLazy<Regex> = SyncLazy::new(|| {
                Regex::new(r"^(([0-9]+(\.[0-9]+)?)%)?(=)?([0-9]+|(minecraft:)?[a-zA-Z_]+)(:([0-9]+)|\[(([a-zA-Z_]+=[a-zA-Z0-9]+,?)+?)\])?((\|([^|]*?)){1,4})?$").unwrap()
            });

            let pattern_match = RE
                .captures(part)
                .ok_or_else(|| PatternParseError::InvalidPattern(part.to_owned()))?;

            let block = if pattern_match.get(4).is_some() {
                Block::from_id(
                    pattern_match
                        .get(5)
                        .map_or("0", |m| m.as_str())
                        .parse::<u32>()
                        .unwrap(),
                )
            } else {
                let block_name = pattern_match
                    .get(5)
                    .unwrap()
                    .as_str()
                    .trim_start_matches("minecraft:");
                Block::from_name(block_name)
                    .ok_or_else(|| PatternParseError::UnknownBlock(part.to_owned()))?
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
        let mut random = rng.gen_range(0.0..weight_sum);

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

        Block::from_id(selected.block_id)
    }
}

struct ChunkChangedRecord {
    chunk_x: i32,
    chunk_z: i32,
    block_count: usize,
}

struct WorldEditOperation {
    pub records: Vec<ChunkChangedRecord>,
    x_range: RangeInclusive<i32>,
    y_range: RangeInclusive<i32>,
    z_range: RangeInclusive<i32>,
}

impl WorldEditOperation {
    fn new(first_pos: BlockPos, second_pos: BlockPos) -> WorldEditOperation {
        let start_pos = first_pos.min(second_pos);
        let end_pos = first_pos.max(second_pos);

        let mut records: Vec<ChunkChangedRecord> = Vec::new();

        for chunk_x in (start_pos.x >> 4)..=(end_pos.x >> 4) {
            for chunk_z in (start_pos.z >> 4)..=(end_pos.z >> 4) {
                records.push(ChunkChangedRecord {
                    chunk_x,
                    chunk_z,
                    block_count: 0,
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

    fn update_block(&mut self, block_pos: BlockPos) {
        let chunk_x = block_pos.x >> 4;
        let chunk_z = block_pos.z >> 4;

        if let Some(packet) = self
            .records
            .iter_mut()
            .find(|c| c.chunk_x == chunk_x && c.chunk_z == chunk_z)
        {
            packet.block_count += 1;
        }
    }

    fn blocks_updated(&self) -> usize {
        let mut blocks_updated = 0;

        for record in &self.records {
            blocks_updated += record.block_count;
        }

        blocks_updated
    }

    fn x_range(&self) -> RangeInclusive<i32> {
        self.x_range.clone()
    }
    fn y_range(&self) -> RangeInclusive<i32> {
        self.y_range.clone()
    }
    fn z_range(&self) -> RangeInclusive<i32> {
        self.z_range.clone()
    }
}

fn ray_trace_block(
    world: &impl World,
    mut pos: PlayerPos,
    start_pitch: f64,
    start_yaw: f64,
    max_distance: f64,
) -> Option<BlockPos> {
    let check_distance = 0.2;

    // Player view height
    pos.y += 1.65;
    let rot_x = (start_yaw + 90.0) % 360.0;
    let rot_y = start_pitch * -1.0;
    let h = check_distance * rot_y.to_radians().cos();

    let offset_x = h * rot_x.to_radians().cos();
    let offset_y = check_distance * rot_y.to_radians().sin();
    let offset_z = h * rot_x.to_radians().sin();

    let mut current_distance = 0.0;

    while current_distance < max_distance {
        let block_pos = pos.block_pos();
        let block = world.get_block(block_pos);

        if !matches!(block, Block::Air {}) {
            return Some(block_pos);
        }

        pos.x += offset_x;
        pos.y += offset_y;
        pos.z += offset_z;
        current_distance += check_distance;
    }

    None
}

fn worldedit_send_operation(plot: &mut PlotWorld, operation: WorldEditOperation) {
    for packet in operation.records {
        let chunk = match plot.get_chunk(packet.chunk_x, packet.chunk_z) {
            Some(chunk) => chunk,
            None => continue,
        };
        let chunk_data = chunk.encode_packet();
        for player in &mut plot.packet_senders {
            player.send_packet(&chunk_data);
        }
    }
}

fn worldedit_start_operation(player: &mut Player) -> WorldEditOperation {
    let first_pos = player.first_position.unwrap();
    let second_pos = player.second_position.unwrap();
    WorldEditOperation::new(first_pos, second_pos)
}

fn execute_set(ctx: CommandExecuteContext<'_>) {
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
                    operation.update_block(block_pos);
                }
            }
        }
    }

    let blocks_updated = operation.blocks_updated();
    worldedit_send_operation(ctx.plot, operation);

    ctx.player.send_worldedit_message(&format!(
        "Operation completed: {} block(s) affected ({:?})",
        blocks_updated,
        start_time.elapsed()
    ));
}

fn execute_replace(ctx: CommandExecuteContext<'_>) {
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
                        operation.update_block(block_pos);
                    }
                }
            }
        }
    }

    let blocks_updated = operation.blocks_updated();
    worldedit_send_operation(ctx.plot, operation);

    ctx.player.send_worldedit_message(&format!(
        "Operation completed: {} block(s) affected ({:?})",
        blocks_updated,
        start_time.elapsed()
    ));
}

fn execute_count(ctx: CommandExecuteContext<'_>) {
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

fn create_clipboard(
    plot: &mut PlotWorld,
    origin: BlockPos,
    first_pos: BlockPos,
    second_pos: BlockPos,
) -> WorldEditClipboard {
    let start_pos = first_pos.min(second_pos);
    let end_pos = first_pos.max(second_pos);
    let size_x = (end_pos.x - start_pos.x) as u32 + 1;
    let size_y = (end_pos.y - start_pos.y) as u32 + 1;
    let size_z = (end_pos.z - start_pos.z) as u32 + 1;
    let offset = origin - start_pos;
    let mut cb = WorldEditClipboard {
        offset_x: offset.x,
        offset_y: offset.y,
        offset_z: offset.z,
        size_x,
        size_y,
        size_z,
        data: PalettedBitBuffer::with_entries((size_x * size_y * size_z) as usize),
        block_entities: HashMap::new(),
    };
    let mut i = 0;
    for y in start_pos.y..=end_pos.y {
        for z in start_pos.z..=end_pos.z {
            for x in start_pos.x..=end_pos.x {
                let pos = BlockPos::new(x, y, z);
                let id = plot.get_block_raw(pos);
                let block = plot.get_block(BlockPos::new(x, y, z));
                if block.has_block_entity() {
                    if let Some(block_entity) = plot.get_block_entity(pos) {
                        cb.block_entities
                            .insert(pos - start_pos, block_entity.clone());
                    }
                }
                cb.data.set_entry(i, id);
                i += 1;
            }
        }
    }
    cb
}

fn clear_area(plot: &mut PlotWorld, first_pos: BlockPos, second_pos: BlockPos) {
    let start_pos = first_pos.min(second_pos);
    let end_pos = first_pos.max(second_pos);
    for y in start_pos.y..=end_pos.y {
        for z in start_pos.z..=end_pos.z {
            for x in start_pos.x..=end_pos.x {
                plot.set_block_raw(BlockPos::new(x, y, z), 0);
            }
        }
    }
    // Send modified chunks
    for chunk_x in (start_pos.x >> 4)..=(end_pos.x >> 4) {
        for chunk_z in (start_pos.z >> 4)..=(end_pos.z >> 4) {
            if let Some(chunk) = plot.get_chunk(chunk_x, chunk_z) {
                let chunk_data = chunk.encode_packet();
                for player in &mut plot.packet_senders {
                    player.send_packet(&chunk_data);
                }
            }
        }
    }
}

fn paste_clipboard(plot: &mut PlotWorld, cb: &WorldEditClipboard, pos: BlockPos, ignore_air: bool) {
    let offset_x = pos.x - cb.offset_x;
    let offset_y = pos.y - cb.offset_y;
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
                let entry = cb.data.get_entry(i);
                i += 1;
                if ignore_air && entry == 0 {
                    continue;
                }
                plot.set_block_raw(BlockPos::new(x, y, z), entry);
            }
        }
    }
    // Calculate the ranges of chunks that might have been modified
    let chunk_x_range = offset_x >> 4..=(offset_x + cb.size_x as i32) >> 4;
    let chunk_z_range = offset_z >> 4..=(offset_z + cb.size_z as i32) >> 4;
    for chunk_x in chunk_x_range {
        for chunk_z in chunk_z_range.clone() {
            if let Some(chunk) = plot.get_chunk(chunk_x, chunk_z) {
                let chunk_data = chunk.encode_packet();
                for player in &mut plot.packet_senders {
                    player.send_packet(&chunk_data);
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

fn capture_undo(
    plot: &mut PlotWorld,
    player: &mut Player,
    first_pos: BlockPos,
    second_pos: BlockPos,
) {
    let origin = first_pos.min(second_pos);
    let cb = create_clipboard(plot, origin, first_pos, second_pos);
    let undo = WorldEditUndo {
        clipboards: vec![cb],
        pos: origin,
        plot_x: plot.x,
        plot_z: plot.z,
    };

    player.worldedit_undo.push(undo);
    player.worldedit_redo.clear();
}

fn execute_copy(mut ctx: CommandExecuteContext<'_>) {
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

fn execute_cut(mut ctx: CommandExecuteContext<'_>) {
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

fn execute_move(mut ctx: CommandExecuteContext<'_>) {
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

fn execute_paste(ctx: CommandExecuteContext<'_>) {
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

fn execute_load(mut ctx: CommandExecuteContext<'_>) {
    let start_time = Instant::now();

    let file_name = ctx.arguments[0].unwrap_string();

    let clipboard = load_schematic(file_name);
    match clipboard {
        Some(cb) => {
            ctx.player.worldedit_clipboard = Some(cb);
            ctx.player.send_worldedit_message(&format!(
                "The schematic was loaded to your clipboard. Do //paste to birth it into the world. ({:?})",
                start_time.elapsed()
            ));
        }
        None => {
            error!("There was an error loading a schematic.");
            ctx.player
                .send_error_message("There was an error loading the schematic.");
        }
    }
}

fn execute_save(ctx: CommandExecuteContext<'_>) {
    let start_time = Instant::now();

    let file_name = ctx.arguments[0].unwrap_string();
    let clipboard = ctx.player.worldedit_clipboard.as_ref().unwrap();

    match save_schematic(file_name, clipboard) {
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

fn execute_stack(ctx: CommandExecuteContext<'_>) {
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

fn execute_undo(mut ctx: CommandExecuteContext<'_>) {
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
    let mut redo = WorldEditUndo {
        clipboards: undo.clipboards.iter().map(|clipboard| {
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
            create_clipboard(&mut ctx.plot, undo.pos, first_pos, second_pos)
        }).collect(),
        ..undo
    };
    for clipboard in &undo.clipboards {
        paste_clipboard(ctx.plot, clipboard, undo.pos, false);
    }
    ctx.player.worldedit_redo.push(redo);
}

fn execute_redo(mut ctx: CommandExecuteContext<'_>) {
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
    let mut undo = WorldEditUndo {
        clipboards: redo.clipboards.iter().map(|clipboard| {
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
            create_clipboard(&mut ctx.plot, redo.pos, first_pos, second_pos)
        }).collect(),
        ..redo
    };
    for clipboard in &redo.clipboards {
        paste_clipboard(ctx.plot, clipboard, redo.pos, false);
    }
    ctx.player.worldedit_undo.push(undo);
}

fn execute_sel(ctx: CommandExecuteContext<'_>) {
    let player = ctx.player;
    player.first_position = None;
    player.second_position = None;
    player.send_worldedit_message("Selection cleared.");
    player.worldedit_send_cui("s|cuboid");
}

fn execute_pos1(ctx: CommandExecuteContext<'_>) {
    let pos = ctx.player.pos.block_pos();
    ctx.player.worldedit_set_first_position(pos);
}

fn execute_pos2(ctx: CommandExecuteContext<'_>) {
    let pos = ctx.player.pos.block_pos();
    ctx.player.worldedit_set_second_position(pos);
}

fn execute_hpos1(mut ctx: CommandExecuteContext<'_>) {
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

fn execute_hpos2(mut ctx: CommandExecuteContext<'_>) {
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

fn expand_selection(player: &mut Player, amount: BlockPos, contract: bool) {
    let mut p1 = player.first_position.unwrap();
    let mut p2 = player.second_position.unwrap();

    fn get_pos_axis(pos: &mut BlockPos, axis: u8) -> &mut i32 {
        match axis {
            0 => &mut pos.x,
            1 => &mut pos.y,
            2 => &mut pos.z,
            _ => unreachable!(),
        }
    }

    let mut expand_axis = |axis: u8| {
        let amount = *get_pos_axis(&mut amount.clone(), axis);
        let p1 = get_pos_axis(&mut p1, axis);
        let p2 = get_pos_axis(&mut p2, axis);
        #[allow(clippy::comparison_chain)]
        if amount > 0 {
            if (p1 > p2) ^ contract {
                *p1 += amount;
            } else {
                *p2 += amount;
            }
        } else if amount < 0 {
            if (p1 < p2) ^ contract {
                *p1 += amount;
            } else {
                *p2 += amount;
            }
        }
    };

    for axis in 0..=2 {
        expand_axis(axis);
    }

    if Some(p1) != player.first_position {
        player.worldedit_set_first_position(p1);
    }
    if Some(p2) != player.second_position {
        player.worldedit_set_second_position(p2);
    }
}

fn execute_expand(ctx: CommandExecuteContext<'_>) {
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

fn execute_contract(ctx: CommandExecuteContext<'_>) {
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

fn execute_shift(ctx: CommandExecuteContext<'_>) {
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

fn execute_flip(mut ctx: CommandExecuteContext<'_>) {
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

    let mut newcpdata = PalettedBitBuffer::with_entries((volume) as usize);

    let mut c_x = 0;
    let mut c_y = 0;
    let mut c_z = 0;
    for i in 0..volume {
        let BlockPos { x: n_x, y: n_y, z: n_z } = flip_pos(BlockPos::new(c_x, c_y, c_z));
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

fn execute_rotate(mut ctx: CommandExecuteContext<'_>) {
    let start_time = Instant::now();
    let rotate_amt = ctx.arguments[0].unwrap_uint();
    let rotate_amt = match rotate_amt % 360 {
        0 => {
            ctx.player.send_worldedit_message("Successfully rotated by 0! That took a lot of work.");
            return;
        }
        90 => RotateAmt::Rotate90,
        180 => RotateAmt::Rotate180,
        270 => RotateAmt::Rotate270,
        _ => {
            ctx.player.send_error_message("Rotate amount must be a multiple of 90.");
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

    let rotate_pos = |pos: BlockPos| {
        match rotate_amt {
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
                z: n_size_z as i32 - 1 - pos.x
            },
        }
    };

    let mut newcpdata = PalettedBitBuffer::with_entries((volume) as usize);

    let mut c_x = 0;
    let mut c_y = 0;
    let mut c_z = 0;
    for i in 0..volume {
        let BlockPos { x: n_x, y: n_y, z: n_z } = rotate_pos(BlockPos::new(c_x, c_y, c_z));
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

fn execute_help(mut ctx: CommandExecuteContext<'_>) {
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

fn execute_up(ctx: CommandExecuteContext<'_>) {
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

fn execute_rstack(ctx: CommandExecuteContext<'_>) {
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

fn execute_unimplemented(_ctx: CommandExecuteContext<'_>) {
    unimplemented!("Unimplimented worldedit command");
}
