use super::argument_parser::*;

#[derive(Clone)]
pub enum ArgumentType {
    String,
    Integer { min: i32, max: i32 },
    Float { min: f32, max: f32 },
    Boolean,
    Player,
    Direction,
    Vec3,
    ColumnPos,
    Container,
    Pattern,
    Mask,
    DirectionExt,
    BlockPos,
    GreedyString,
    Flags { flags: Vec<FlagSpec> },
}

#[derive(Default, Clone)]
pub struct ArgumentTypeFlagBuilder {
    flags: Vec<FlagSpec>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct FlagSpec {
    pub(super) short: Option<char>,
    pub(super) long: String,
    pub(super) description: Option<String>,
}

pub(super) struct OptionChar(Option<char>);

pub(super) struct OptionString(Option<String>);

impl ArgumentType {
    pub(super) fn parse<'a>(&self, input: &'a str) -> ArgumentParseResult<'a> {
        match self {
            ArgumentType::String => parse_string(input),
            ArgumentType::Integer { min, max } => {
                let (value, rest) = parse_integer(input)?;
                if let Ok(value) = value.as_integer() {
                    if !(*min..=*max).contains(&value) {
                        return Err(());
                    }
                }
                Ok((value, rest))
            }
            ArgumentType::Float { min, max } => {
                let (value, rest) = parse_float(input)?;
                if let Ok(value) = value.as_float() {
                    if !(*min..=*max).contains(&value) {
                        return Err(());
                    }
                }
                Ok((value, rest))
            }
            ArgumentType::Boolean => parse_boolean(input),
            ArgumentType::Player => parse_player(input),
            ArgumentType::Direction => parse_direction(input),
            ArgumentType::Vec3 => parse_vec3(input),
            ArgumentType::ColumnPos => parse_column_pos(input),
            ArgumentType::Container => parse_container(input),
            ArgumentType::Pattern => parse_pattern(input),
            ArgumentType::Mask => parse_mask(input),
            ArgumentType::DirectionExt => parse_direction_ext(input),
            ArgumentType::BlockPos => parse_block_pos(input),
            ArgumentType::GreedyString => parse_greedy_string(input),
            ArgumentType::Flags { flags: specs } => parse_flags(input, specs),
        }
    }

    pub fn string() -> Self {
        ArgumentType::String
    }

    pub fn integer(min: i32, max: i32) -> Self {
        ArgumentType::Integer { min, max }
    }

    pub fn float(min: f32, max: f32) -> Self {
        ArgumentType::Float { min, max }
    }

    pub fn boolean() -> Self {
        ArgumentType::Boolean
    }

    pub fn player() -> Self {
        ArgumentType::Player
    }

    pub fn direction() -> Self {
        ArgumentType::Direction
    }

    pub fn vec3() -> Self {
        ArgumentType::Vec3
    }

    pub fn column_pos() -> Self {
        ArgumentType::ColumnPos
    }

    pub fn container() -> Self {
        ArgumentType::Container
    }

    pub fn pattern() -> Self {
        ArgumentType::Pattern
    }

    pub fn mask() -> Self {
        ArgumentType::Mask
    }

    pub fn direction_ext() -> Self {
        ArgumentType::DirectionExt
    }

    pub fn block_pos() -> Self {
        ArgumentType::BlockPos
    }

    pub fn greedy_string() -> Self {
        ArgumentType::GreedyString
    }

    pub fn flags() -> ArgumentTypeFlagBuilder {
        ArgumentTypeFlagBuilder::default()
    }
}

impl ArgumentTypeFlagBuilder {
    pub(super) fn add(
        mut self,
        short: impl Into<OptionChar>,
        long: &str,
        description: impl Into<OptionString>,
    ) -> Self {
        self.flags.push(FlagSpec {
            short: short.into().0,
            long: long.to_string(),
            description: description.into().0,
        });
        self
    }

    pub(super) fn build(self) -> ArgumentType {
        ArgumentType::Flags { flags: self.flags }
    }
}

impl From<ArgumentTypeFlagBuilder> for ArgumentType {
    fn from(value: ArgumentTypeFlagBuilder) -> Self {
        value.build()
    }
}

impl From<char> for OptionChar {
    fn from(c: char) -> Self {
        OptionChar(Some(c))
    }
}

impl From<Option<char>> for OptionChar {
    fn from(o: Option<char>) -> Self {
        OptionChar(o)
    }
}

impl From<&str> for OptionString {
    fn from(s: &str) -> Self {
        OptionString(Some(s.to_string()))
    }
}

impl From<Option<&str>> for OptionString {
    fn from(o: Option<&str>) -> Self {
        OptionString(o.map(|s| s.to_string()))
    }
}
