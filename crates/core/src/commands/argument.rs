use super::{
    argument_parser, argument_suggestions,
    suggestions::SuggestionSource,
    value::{BlockCoordinates, Value},
};
use crate::worldedit::{mask::WorldEditMask, pattern::WorldEditPattern};
use mchprs_commands::{Argument, Reader, SuggestionsBuilder, SyntaxError};
use mchprs_network::packets::clientbound::CDeclareCommandsNodeParser as Parser;

pub(super) enum ClientArgument {
    Client(Parser),
    AskServer(Parser),
    Opaque,
}

#[derive(Clone, PartialEq)]
pub enum ArgumentType {
    Word,
    Integer { min: i32, max: i32 },
    Float { min: f32, max: f32 },
    PlayerTarget,
    Direction,
    Vec3,
    PlotPos,
    ContainerType,
    Pattern,
    Mask,
    Replacement,
    DirectionWithDiagonals,
    BlockPos,
    SchematicFile,
    GreedyString,
    Flags { flags: Vec<FlagSpec> },
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct FlagSpec {
    pub(super) short: Option<char>,
    pub(super) long: String,
    pub(super) description: String,
}

impl ArgumentType {
    pub fn integer(min: i32, max: i32) -> Self {
        ArgumentType::Integer { min, max }
    }

    pub fn float(min: f32, max: f32) -> Self {
        ArgumentType::Float { min, max }
    }

    pub(super) fn client_argument(&self) -> ClientArgument {
        match self {
            Self::Integer { min, max } => ClientArgument::Client(Parser::Integer(*min, *max)),
            Self::Float { min, max } => ClientArgument::Client(Parser::Float(*min, *max)),
            Self::Word => ClientArgument::Client(Parser::String(0)),
            Self::GreedyString => ClientArgument::Client(Parser::String(2)),
            Self::Vec3 => ClientArgument::Client(Parser::Vec3),
            Self::BlockPos => ClientArgument::Client(Parser::BlockPos),
            Self::Flags { .. } => ClientArgument::AskServer(Parser::String(2)),
            Self::Direction
            | Self::DirectionWithDiagonals
            | Self::ContainerType
            | Self::SchematicFile => ClientArgument::AskServer(Parser::String(0)),
            Self::PlotPos => ClientArgument::AskServer(Parser::ColumnPos),
            Self::Pattern | Self::Mask | Self::Replacement | Self::PlayerTarget => {
                ClientArgument::Opaque
            }
        }
    }

    pub(super) fn suggest(
        &self,
        builder: &mut SuggestionsBuilder<'_, '_>,
    ) -> Option<SuggestionSource> {
        match self {
            Self::Direction => builder.suggest_matching(
                argument_parser::DIRECTIONS
                    .iter()
                    .flat_map(|(names, _)| *names),
            ),
            Self::DirectionWithDiagonals => builder.suggest_matching(
                argument_parser::DIRECTIONS_WITH_DIAGONALS
                    .iter()
                    .flat_map(|(names, _)| *names),
            ),
            Self::ContainerType => builder.suggest_matching(
                argument_parser::CONTAINER_TYPES
                    .iter()
                    .map(|(name, _)| name),
            ),
            Self::PlayerTarget => {
                builder.suggest_matching(["@s"]);
                return Some(SuggestionSource::PlayerNames);
            }
            Self::Flags { flags } => argument_suggestions::flags(flags, builder),
            Self::Pattern => argument_suggestions::quoted(self, builder, WorldEditPattern::suggest),
            Self::Mask => argument_suggestions::quoted(self, builder, WorldEditMask::suggest),
            Self::Replacement => argument_suggestions::quoted(self, builder, |builder| {
                WorldEditPattern::suggest(builder);
                WorldEditMask::suggest(builder);
            }),
            Self::Vec3 | Self::BlockPos => {
                argument_suggestions::coordinates(self, &["~ ~ ~", "^ ^ ^"], builder)
            }
            Self::PlotPos => argument_suggestions::coordinates(self, &["~ ~"], builder),
            Self::SchematicFile => return Some(SuggestionSource::SchematicFiles),
            Self::Word | Self::GreedyString | Self::Integer { .. } | Self::Float { .. } => {}
        }
        None
    }
}

impl Argument for ArgumentType {
    type Value = Value;

    fn consumes_remaining(&self) -> bool {
        matches!(self, Self::GreedyString | Self::Flags { .. })
    }

    fn parse(&self, reader: &mut Reader<'_>) -> Result<Value, SyntaxError> {
        match self {
            ArgumentType::Word | ArgumentType::SchematicFile => Ok(Value::String(reader.word())),
            ArgumentType::PlayerTarget => argument_parser::parse_player(reader),
            ArgumentType::Integer { min, max } => reader.number(*min, *max).map(Value::Integer),
            ArgumentType::Float { min, max } => reader.number(*min, *max).map(Value::Float),
            ArgumentType::Direction => argument_parser::parse_direction(reader),
            ArgumentType::Vec3 => argument_parser::parse_vec3(reader, true).map(Value::Position),
            ArgumentType::PlotPos => argument_parser::parse_plot_pos(reader),
            ArgumentType::ContainerType => argument_parser::parse_container(reader),
            ArgumentType::Pattern => argument_parser::parse_pattern(reader),
            ArgumentType::Mask => argument_parser::parse_mask(reader),
            ArgumentType::Replacement => argument_parser::parse_replacement(reader),
            ArgumentType::DirectionWithDiagonals => {
                argument_parser::parse_direction_with_diagonals(reader)
            }
            ArgumentType::BlockPos => argument_parser::parse_vec3(reader, false)
                .map(|position| Value::BlockPos(BlockCoordinates(position))),
            ArgumentType::GreedyString => Ok(Value::String(reader.greedy())),
            ArgumentType::Flags { flags } => {
                argument_parser::parse_flags(reader, flags).map(Value::Flags)
            }
        }
    }
}
