use crate::{
    commands::{argument::FlagSpec, value::*},
    worldedit::WorldEditPattern,
};
use mchprs_blocks::{block_entities::ContainerType, BlockPos};
use rustc_hash::FxHashSet;
use std::str::FromStr;

pub type ArgumentParseResult<'a> = Result<(Value, &'a str), ()>;

fn skip_whitespace(input: &str) -> &str {
    input.trim_start()
}

pub fn consume_token(input: &str) -> Option<(&str, &str)> {
    let input = skip_whitespace(input);
    if input.is_empty() {
        return None;
    }

    let end = input.find(char::is_whitespace).unwrap_or(input.len());

    Some((&input[..end], &input[end..]))
}

pub fn parse_string(input: &str) -> ArgumentParseResult<'_> {
    let input = skip_whitespace(input);
    if let Some((token, rest)) = consume_token(input) {
        Ok((Value::String(token.to_string()), rest))
    } else {
        Err(())
    }
}

pub fn parse_greedy_string(input: &str) -> ArgumentParseResult<'_> {
    let input = skip_whitespace(input);
    Ok((Value::GreedyString(input.to_string()), ""))
}

pub fn parse_flags<'a>(input: &'a str, flags_specs: &[FlagSpec]) -> ArgumentParseResult<'a> {
    let input = skip_whitespace(input);

    let mut flags = FxHashSet::default();
    let tokens = input.split_whitespace();

    for token in tokens {
        if let Some(long_name) = token.strip_prefix("--") {
            let spec = flags_specs.iter().find(|s| s.long == long_name).ok_or(())?;
            flags.insert(spec.long.clone());
        } else if let Some(flag_chars) = token.strip_prefix('-') {
            for c in flag_chars.chars() {
                let spec = flags_specs.iter().find(|s| s.short == Some(c)).ok_or(())?;
                flags.insert(spec.long.clone());
            }
        } else {
            return Err(());
        }
    }

    Ok((Value::Flags(flags), ""))
}

pub fn parse_integer(input: &str) -> ArgumentParseResult<'_> {
    let input = skip_whitespace(input);
    let (token, rest) = consume_token(input).ok_or(())?;

    let value = token.parse::<i32>().map_err(|_| ())?;

    Ok((Value::Integer(value), rest))
}

pub fn parse_float(input: &str) -> ArgumentParseResult<'_> {
    let input = skip_whitespace(input);
    let (token, rest) = consume_token(input).ok_or(())?;

    let value = token.parse::<f32>().map_err(|_| ())?;

    Ok((Value::Float(value), rest))
}

pub fn parse_boolean(input: &str) -> ArgumentParseResult<'_> {
    let input = skip_whitespace(input);
    let (token, rest) = consume_token(input).ok_or(())?;

    let value = match token.to_lowercase().as_str() {
        "true" | "yes" | "1" | "on" => true,
        "false" | "no" | "0" | "off" => false,
        _ => return Err(()),
    };

    Ok((Value::Boolean(value), rest))
}

pub fn parse_player(input: &str) -> ArgumentParseResult<'_> {
    let input = skip_whitespace(input);
    let (token, rest) = consume_token(input).ok_or(())?;

    Ok((Value::Player(token.to_string()), rest))
}

pub fn parse_vec3(input: &str) -> ArgumentParseResult<'_> {
    let input = skip_whitespace(input);

    let mut coords = Vec::new();
    let mut remaining = input;

    for _ in 0..3 {
        remaining = skip_whitespace(remaining);
        let (token, rest) = consume_token(remaining).ok_or(())?;

        let coord = if token == "~" {
            RelativeCoord::Relative(0.0)
        } else if let Some(offset_str) = token.strip_prefix('~') {
            let offset = offset_str.parse::<f64>().map_err(|_| ())?;
            RelativeCoord::Relative(offset)
        } else {
            let val = token.parse::<f64>().map_err(|_| ())?;
            RelativeCoord::Absolute(val)
        };

        coords.push(coord);
        remaining = rest;
    }

    Ok((
        Value::Vec3(Vec3 {
            x: coords[0],
            y: coords[1],
            z: coords[2],
        }),
        remaining,
    ))
}

pub fn parse_column_pos(input: &str) -> ArgumentParseResult<'_> {
    let input = skip_whitespace(input);

    let mut coords = Vec::new();
    let mut remaining = input;

    for _ in 0..2 {
        remaining = skip_whitespace(remaining);
        let (token, rest) = consume_token(remaining).ok_or(())?;

        let coord = if token == "~" {
            RelativeCoord::Relative(0)
        } else if let Some(offset_str) = token.strip_prefix('~') {
            let offset = offset_str.parse::<i32>().map_err(|_| ())?;
            RelativeCoord::Relative(offset)
        } else {
            let val = token.parse::<i32>().map_err(|_| ())?;
            RelativeCoord::Absolute(val)
        };

        coords.push(coord);
        remaining = rest;
    }

    Ok((
        Value::ColumnPos(ColumnPos {
            x: coords[0],
            z: coords[1],
        }),
        remaining,
    ))
}

pub fn parse_container(input: &str) -> ArgumentParseResult<'_> {
    let input = skip_whitespace(input);
    let (token, rest) = consume_token(input).ok_or(())?;

    let container_type = match token {
        "barrel" => ContainerType::Barrel,
        "furnace" => ContainerType::Furnace,
        "hopper" => ContainerType::Hopper,
        _ => return Err(()),
    };

    Ok((Value::Container(container_type), rest))
}

pub fn parse_pattern(input: &str) -> ArgumentParseResult<'_> {
    let input = skip_whitespace(input);
    let (token, rest) = consume_token(input).ok_or(())?;

    let pattern = WorldEditPattern::from_str(token).map_err(|_| ())?;

    Ok((Value::Pattern(pattern), rest))
}

pub fn parse_mask(input: &str) -> ArgumentParseResult<'_> {
    let input = skip_whitespace(input);
    let (token, rest) = consume_token(input).ok_or(())?;

    let mask = WorldEditPattern::from_str(token).map_err(|_| ())?;

    Ok((Value::Mask(mask), rest))
}

pub fn parse_direction(input: &str) -> ArgumentParseResult<'_> {
    let input = skip_whitespace(input);
    let (token, rest) = consume_token(input).ok_or(())?;

    let dir = match token.to_lowercase().as_str() {
        "me" => Direction::Me,
        "l" | "left" => Direction::Left,
        "r" | "right" => Direction::Right,
        "u" | "up" => Direction::Up,
        "d" | "down" => Direction::Down,
        "n" | "north" => Direction::North,
        "s" | "south" => Direction::South,
        "e" | "east" => Direction::East,
        "w" | "west" => Direction::West,
        _ => return Err(()),
    };

    Ok((Value::Direction(dir), rest))
}

pub fn parse_direction_ext(input: &str) -> ArgumentParseResult<'_> {
    let input = skip_whitespace(input);
    let (token, rest) = consume_token(input).ok_or(())?;

    let dir_vec = match token.to_lowercase().as_str() {
        "me" => DirectionExt::Me,
        "l" | "left" => DirectionExt::Left,
        "r" | "right" => DirectionExt::Right,
        "u" | "up" => DirectionExt::Up,
        "d" | "down" => DirectionExt::Down,
        "n" | "north" => DirectionExt::North,
        "s" | "south" => DirectionExt::South,
        "e" | "east" => DirectionExt::East,
        "w" | "west" => DirectionExt::West,
        "lu" | "leftup" => DirectionExt::LeftUp,
        "ld" | "leftdown" => DirectionExt::LeftDown,
        "ru" | "rightup" => DirectionExt::RightUp,
        "rd" | "rightdown" => DirectionExt::RightDown,
        "nu" | "northup" => DirectionExt::NorthUp,
        "nd" | "northdown" => DirectionExt::NorthDown,
        "su" | "southup" => DirectionExt::SouthUp,
        "sd" | "southdown" => DirectionExt::SouthDown,
        "eu" | "eastup" => DirectionExt::EastUp,
        "ed" | "eastdown" => DirectionExt::EastDown,
        "wu" | "westup" => DirectionExt::WestUp,
        "wd" | "westdown" => DirectionExt::WestDown,
        _ => return Err(()),
    };

    Ok((Value::DirectionExt(dir_vec), rest))
}

pub fn parse_block_pos(input: &str) -> ArgumentParseResult<'_> {
    let input = skip_whitespace(input);

    let mut coords = Vec::new();
    let mut remaining = input;

    for _ in 0..3 {
        remaining = skip_whitespace(remaining);
        let (token, rest) = consume_token(remaining).ok_or(())?;

        let coord = token.parse::<i32>().map_err(|_| ())?;

        coords.push(coord);
        remaining = rest;
    }

    Ok((
        Value::BlockPos(BlockPos::new(coords[0], coords[1], coords[2])),
        remaining,
    ))
}
