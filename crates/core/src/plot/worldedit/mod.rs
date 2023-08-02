//! [Worldedit](https://github.com/EngineHub/WorldEdit) and [RedstoneTools](https://github.com/paulikauro/RedstoneTools) implementation

mod execute;
mod schematic;

use super::{Plot, PlotWorld};
use crate::player::{PacketSender, Player, PlayerPos};
use crate::world::storage::PalettedBitBuffer;
use crate::world::World;
use execute::*;
use mchprs_blocks::block_entities::{BlockEntity, ContainerType};
use mchprs_blocks::blocks::Block;
use mchprs_blocks::{BlockFacing, BlockPos};
use mchprs_utils::map;
use once_cell::sync::Lazy;
use rand::Rng;
use regex::Regex;
use rustc_hash::FxHashMap;
use std::collections::HashMap;
use std::fmt;
use std::ops::RangeInclusive;
use std::str::FromStr;

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
        args.append(&mut alias);
        &COMMANDS[command]
    } else {
        return false;
    };

    let wea = player.has_permission("plots.worldedit.bypass");
    if !wea {
        if let Some(owner) = plot.owner {
            if owner != player.uuid {
                // tried to worldedit on plot that wasn't theirs
                player.send_no_permission_message();
                return true;
            }
        } else {
            // tried to worldedit on unclaimed plot
            player.send_no_permission_message();
            return true;
        }
    }

    if !command.permission_node.is_empty() && !player.has_permission(command.permission_node) {
        player.send_no_permission_message();
        return true;
    }

    if command.requires_positions {
        let plot_x = plot.world.x;
        let plot_z = plot.world.z;
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

    if command.requires_clipboard && player.worldedit_clipboard.is_none() {
        player.send_error_message("Your clipboard is empty. Use //copy first.");
        return true;
    }

    let flag_descs = command.flags;

    let mut ctx_flags = Vec::new();
    let mut arg_removal_idxs = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if arg.starts_with('-') {
            let mut with_argument = false;
            let flags = arg.chars();
            for flag in flags.skip(1) {
                if with_argument {
                    player.send_error_message("Flag with argument must be last in grouping");
                    return true;
                }
                let flag_desc = if let Some(desc) = flag_descs.iter().find(|d| d.letter == flag) {
                    desc
                } else {
                    player.send_error_message(&format!("Unknown flag: {}", flag));
                    return true;
                };
                arg_removal_idxs.push(i);
                if flag_desc.argument_type.is_some() {
                    arg_removal_idxs.push(i + 1);
                    with_argument = true;
                }
                ctx_flags.push(flag);
            }
        }
    }

    for idx in arg_removal_idxs.iter().rev() {
        args.remove(*idx);
    }

    let arg_descs = command.arguments;

    if args.len() > arg_descs.len() {
        player.send_error_message("Too many arguments.");
        return true;
    }

    let mut arguments = Vec::new();
    for (i, arg_desc) in arg_descs.iter().enumerate() {
        let arg = args.get(i).copied();
        match Argument::parse(player, arg_desc, arg) {
            Ok(default_arg) => arguments.push(default_arg),
            Err(err) => {
                player.send_error_message(&err.to_string());
                return true;
            }
        }
    }
    if command.mutates_world {
        plot.reset_redpiler();
    }
    let ctx = CommandExecuteContext {
        plot: &mut plot.world,
        player: &mut plot.players[player_idx],
        arguments,
        flags: ctx_flags,
    };
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
    ContainerType,
}

#[derive(Debug, Clone)]
enum Argument {
    UnsignedInteger(u32),
    Direction(BlockFacing),
    DirectionVector(BlockPos),
    Pattern(WorldEditPattern),
    Mask(WorldEditPattern),
    String(String),
    ContainerType(ContainerType),
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

    fn unwrap_container_type(&self) -> ContainerType {
        match self {
            Argument::ContainerType(val) => *val,
            _ => panic!("Container type must be one of [barrel, furnace, hopper]"),
        }
    }

    fn get_default(player: &Player, desc: &ArgumentDescription) -> ArgumentParseResult {
        if let Some(default) = &desc.default {
            return Ok(default.clone());
        }

        let arg_type = desc.argument_type;
        match arg_type {
            ArgumentType::Direction | ArgumentType::DirectionVector => {
                Argument::parse(player, desc, Some("me"))
            }
            ArgumentType::UnsignedInteger => Ok(Argument::UnsignedInteger(1)),
            _ => Err(ArgumentParseError::new(
                arg_type,
                "argument can't be inferred",
            )),
        }
    }

    fn parse(
        player: &Player,
        desc: &ArgumentDescription,
        arg: Option<&str>,
    ) -> ArgumentParseResult {
        if arg.is_none() {
            return Argument::get_default(player, desc);
        }
        let arg = arg.unwrap();
        let arg_type = desc.argument_type;
        match arg_type {
            ArgumentType::Direction => {
                let player_facing = player.get_facing();
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
                let player_facing = player.get_facing();
                if arg == "me" {
                    vec = player_facing.offset_pos(vec, 1);
                    if !matches!(player_facing, BlockFacing::Down | BlockFacing::Up) {
                        let pitch = player.pitch;
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
            ArgumentType::ContainerType => match arg.parse::<ContainerType>() {
                Ok(ty) => Ok(Argument::ContainerType(ty)),
                Err(_) => Err(ArgumentParseError::new(
                    arg_type,
                    "error parsing container type",
                )),
            },
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
    mutates_world: bool,
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
            mutates_world: true,
        }
    }
}

static COMMANDS: Lazy<HashMap<&'static str, WorldeditCommand>> = Lazy::new(|| {
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
        "ascend" => WorldeditCommand {
            execute_fn: execute_ascend,
            description: "Go up a floor",
            arguments: &[
                argument!("levels", UnsignedInteger, "# of levels to ascend")
            ],
            permission_node: "worldedit.navigation.ascend",
            mutates_world: false,
            ..Default::default()
        },
        "descend" => WorldeditCommand {
            execute_fn: execute_descend,
            description: "Go down a floor",
            arguments: &[
                argument!("levels", UnsignedInteger, "# of levels to descend")
            ],
            permission_node: "worldedit.navigation.descend",
            mutates_world: false,
            ..Default::default()
        },
        "/pos1" => WorldeditCommand {
            execute_fn: execute_pos1,
            description: "Set position 1",
            permission_node: "worldedit.selection.pos",
            mutates_world: false,
            ..Default::default()
        },
        "/pos2" => WorldeditCommand {
            execute_fn: execute_pos2,
            description: "Set position 2",
            permission_node: "worldedit.selection.pos",
            mutates_world: false,
            ..Default::default()
        },
        "/hpos1" => WorldeditCommand {
            execute_fn: execute_hpos1,
            description: "Set position 1 to targeted block",
            permission_node: "worldedit.selection.hpos",
            mutates_world: false,
            ..Default::default()
        },
        "/hpos2" => WorldeditCommand {
            execute_fn: execute_hpos2,
            description: "Set position 2 to targeted block",
            permission_node: "worldedit.selection.hpos",
            mutates_world: false,
            ..Default::default()
        },
        "/sel" => WorldeditCommand {
            execute_fn: execute_sel,
            description: "Choose a region selector",
            mutates_world: false,
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
            mutates_world: false,
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
            mutates_world: false,
            ..Default::default()
        },
        "/load" => WorldeditCommand {
            arguments: &[
                argument!("name", String, "The file name of the schematic to load")
            ],
            execute_fn: execute_load,
            description: "Loads a schematic file into the clipboard",
            permission_node: "worldedit.clipboard.load",
            mutates_world: false,
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
            mutates_world: false,
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
            mutates_world: false,
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
            mutates_world: false,
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
            mutates_world: false,
            ..Default::default()
        },
        "/flip" => WorldeditCommand {
            arguments: &[
                argument!("direction", Direction, "The direction to flip, defaults to look direction"),
            ],
            requires_clipboard: true,
            execute_fn: execute_flip,
            description: "Flip the contents of the clipboard across the origin",
            mutates_world: false,
            ..Default::default()
        },
        "/rotate" => WorldeditCommand {
            arguments: &[
                argument!("rotateY", UnsignedInteger, "Amount to rotate on the x-axis", 0),
            ],
            requires_clipboard: true,
            execute_fn: execute_rotate,
            description: "Rotate the contents of the clipboard",
            mutates_world: false,
            ..Default::default()
        },
        "/rstack" => WorldeditCommand {
            arguments: &[
                argument!("count", UnsignedInteger, "# of copies to stack"),
                argument!("spacing", UnsignedInteger, "The spacing between each selection", 2),
                argument!("direction", DirectionVector, "The direction to stack")
            ],
            requires_positions: true,
            flags: &[
                flag!('a', None, "Include air blocks"),
                flag!('e', None, "Expand selection")
            ],
            execute_fn: execute_rstack,
            description: "Like //stack but allows the stacked copies to overlap, supports more directions, and more flags",
            permission_node: "redstonetools.rstack",
            ..Default::default()
        },
        "/update" => WorldeditCommand {
            execute_fn: execute_update,
            description: "Updates all blocks in the selection",
            permission_node: "mchprs.we.update",
            requires_positions: true,
            ..Default::default()
        },
        "/help" => WorldeditCommand {
            arguments: &[
                argument!("command", String, "Command to retrieve help for"),
            ],
            execute_fn: execute_help,
            description: "Displays help for WorldEdit commands",
            permission_node: "worldedit.help",
            mutates_world: false,
            ..Default::default()
        },
        "/wand" => WorldeditCommand {
           execute_fn: execute_wand,
           description: "Gives a WorldEdit wand",
           permission_node: "worldedit.wand",
            mutates_world: false,
           ..Default::default()
        },
        "/replacecontainer" => WorldeditCommand {
            arguments: &[
                argument!("from", ContainerType, "The container type to replace"),
                argument!("to", ContainerType, "The container type to replace with"),
            ],
           execute_fn: execute_replace_container,
           description: "Replaces all container types in the selection",
           permission_node: "mchprs.we.replacecontainer",
           requires_positions: true,
           ..Default::default()
        }
    }
});

static ALIASES: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    map! {
        "u" => "up",
        "desc" => "descend",
        "asc" => "ascend",
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
        "/rs" => "/rstack",
        "/rc" => "/replacecontainer"
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
    pub block_entities: FxHashMap<BlockPos, BlockEntity>,
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

impl FromStr for WorldEditPattern {
    type Err = PatternParseError;

    fn from_str(pattern_str: &str) -> PatternParseResult<WorldEditPattern> {
        let mut pattern = WorldEditPattern { parts: Vec::new() };
        for part in pattern_str.split(',') {
            static RE: Lazy<Regex> = Lazy::new(|| {
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
}

impl WorldEditPattern {
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

struct WorldEditOperation {
    blocks_updated: usize,
    x_range: RangeInclusive<i32>,
    y_range: RangeInclusive<i32>,
    z_range: RangeInclusive<i32>,
}

impl WorldEditOperation {
    fn new(first_pos: BlockPos, second_pos: BlockPos) -> WorldEditOperation {
        let start_pos = first_pos.min(second_pos);
        let end_pos = first_pos.max(second_pos);

        let x_range = start_pos.x..=end_pos.x;
        let y_range = start_pos.y..=end_pos.y;
        let z_range = start_pos.z..=end_pos.z;

        WorldEditOperation {
            blocks_updated: 0,
            x_range,
            y_range,
            z_range,
        }
    }

    fn update_block(&mut self) {
        self.blocks_updated += 1;
    }

    fn blocks_updated(&self) -> usize {
        self.blocks_updated
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

pub fn ray_trace_block(
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

fn worldedit_start_operation(player: &mut Player) -> WorldEditOperation {
    let first_pos = player.first_position.unwrap();
    let second_pos = player.second_position.unwrap();
    WorldEditOperation::new(first_pos, second_pos)
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
        data: PalettedBitBuffer::new((size_x * size_y * size_z) as usize, 9),
        block_entities: FxHashMap::default(),
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
    'top_loop: for y in y_range {
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

    // Send block changes before we send block entity data, otherwise it'll be ignored
    plot.flush_block_changes();

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
