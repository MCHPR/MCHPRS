use crate::commands::{argument::FlagSpec, value::*};
use crate::worldedit::{mask::WorldEditMask, pattern::WorldEditPattern};
use mchprs_blocks::block_entities::ContainerType;
use mchprs_commands::{Reader, SyntaxError};
use rustc_hash::FxHashSet;
use std::str::FromStr;

pub const CONTAINER_TYPES: &[(&str, ContainerType)] = &[
    ("barrel", ContainerType::Barrel),
    ("furnace", ContainerType::Furnace),
    ("hopper", ContainerType::Hopper),
];

pub const DIRECTIONS: &[(&[&str], Direction)] = &[
    (&["me"], Direction::Me),
    (&["l", "left"], Direction::Left),
    (&["r", "right"], Direction::Right),
    (&["u", "up"], Direction::Up),
    (&["d", "down"], Direction::Down),
    (&["n", "north"], Direction::North),
    (&["s", "south"], Direction::South),
    (&["e", "east"], Direction::East),
    (&["w", "west"], Direction::West),
];

pub const DIRECTIONS_WITH_DIAGONALS: &[(&[&str], DirectionWithDiagonals)] = &[
    (&["me"], DirectionWithDiagonals::Me),
    (&["l", "left"], DirectionWithDiagonals::Left),
    (&["r", "right"], DirectionWithDiagonals::Right),
    (&["u", "up"], DirectionWithDiagonals::Up),
    (&["d", "down"], DirectionWithDiagonals::Down),
    (&["n", "north"], DirectionWithDiagonals::North),
    (&["s", "south"], DirectionWithDiagonals::South),
    (&["e", "east"], DirectionWithDiagonals::East),
    (&["w", "west"], DirectionWithDiagonals::West),
    (&["lu", "leftup"], DirectionWithDiagonals::LeftUp),
    (&["ld", "leftdown"], DirectionWithDiagonals::LeftDown),
    (&["ru", "rightup"], DirectionWithDiagonals::RightUp),
    (&["rd", "rightdown"], DirectionWithDiagonals::RightDown),
    (&["nu", "northup"], DirectionWithDiagonals::NorthUp),
    (&["nd", "northdown"], DirectionWithDiagonals::NorthDown),
    (&["su", "southup"], DirectionWithDiagonals::SouthUp),
    (&["sd", "southdown"], DirectionWithDiagonals::SouthDown),
    (&["eu", "eastup"], DirectionWithDiagonals::EastUp),
    (&["ed", "eastdown"], DirectionWithDiagonals::EastDown),
    (&["wu", "westup"], DirectionWithDiagonals::WestUp),
    (&["wd", "westdown"], DirectionWithDiagonals::WestDown),
];

pub fn parse_flags(
    reader: &mut Reader<'_>,
    specs: &[FlagSpec],
) -> Result<FxHashSet<String>, SyntaxError> {
    let mut flags = FxHashSet::default();
    while reader.can_read() {
        let token = reader.token()?;
        if let Some(long) = token.strip_prefix("--") {
            let spec = specs
                .iter()
                .find(|spec| spec.long == long)
                .ok_or_else(|| reader.error(format!("Unknown flag: --{long}")))?;
            flags.insert(spec.long.clone());
        } else if let Some(short) = token.strip_prefix('-').filter(|short| !short.is_empty()) {
            for ch in short.chars() {
                let spec = specs
                    .iter()
                    .find(|spec| spec.short == Some(ch))
                    .ok_or_else(|| reader.error(format!("Unknown flag: -{ch}")))?;
                flags.insert(spec.long.clone());
            }
        } else {
            return Err(reader.error(format!("Expected a flag, found {token}")));
        }
        reader.skip_whitespace();
    }
    Ok(flags)
}

fn separator(reader: &mut Reader<'_>, count: usize) -> Result<(), SyntaxError> {
    if reader.peek().is_some_and(|ch| ch != ' ') {
        return Err(reader.error("Expected whitespace between coordinates"));
    }
    if reader.read().is_none() || !reader.can_read() {
        return Err(reader.error(format!("Incomplete (expected {count} coordinates)")));
    }
    Ok(())
}

pub fn parse_vec3(
    reader: &mut Reader<'_>,
    center: bool,
) -> Result<PositionCoordinates, SyntaxError> {
    if reader.peek() == Some('^') {
        let mut local = [0.0; 3];
        for (i, value) in local.iter_mut().enumerate() {
            if i != 0 {
                separator(reader, 3)?;
            }
            if reader.read() != Some('^') {
                return Err(reader.error("Cannot mix local and world coordinates"));
            }
            if reader.peek().is_some_and(|ch| ch != ' ') {
                *value = reader.number(-f64::MAX, f64::MAX)?;
            }
        }
        let [left, up, forward] = local;
        return Ok(PositionCoordinates::ViewAxes { left, up, forward });
    }

    let mut coordinates = [Coordinate::Absolute(0.0); 3];
    for (i, coordinate) in coordinates.iter_mut().enumerate() {
        if i != 0 {
            separator(reader, 3)?;
        }
        let relative = reader.peek() == Some('~');
        if relative {
            reader.read();
        }
        let start = reader.cursor();
        let value = if relative && reader.peek().is_none_or(|ch| ch == ' ') {
            0.0
        } else if !center && !relative {
            reader.number(i32::MIN, i32::MAX)? as f64
        } else {
            reader.number(-f64::MAX, f64::MAX)?
        };
        let integer = !reader.input()[start..reader.cursor()].contains('.');
        *coordinate = if relative {
            Coordinate::Relative(value)
        } else {
            Coordinate::Absolute(
                value
                    + if center && i != 1 && integer {
                        0.5
                    } else {
                        0.0
                    },
            )
        };
    }
    let [x, y, z] = coordinates;
    Ok(PositionCoordinates::WorldAxes { x, y, z })
}

pub fn parse_plot_pos(reader: &mut Reader<'_>) -> Result<Value, SyntaxError> {
    let mut coordinates = [Coordinate::Absolute(0); 2];
    for (i, coordinate) in coordinates.iter_mut().enumerate() {
        if i != 0 {
            separator(reader, 2)?;
        }
        let relative = reader.peek() == Some('~');
        if relative {
            reader.read();
        }
        let value = if relative && reader.peek().is_none_or(|ch| ch == ' ') {
            0
        } else {
            reader.number(i32::MIN, i32::MAX)?
        };
        *coordinate = if relative {
            Coordinate::Relative(value)
        } else {
            Coordinate::Absolute(value)
        };
    }
    Ok(Value::PlotPos(PlotCoordinates {
        x: coordinates[0],
        z: coordinates[1],
    }))
}

pub fn parse_container(reader: &mut Reader<'_>) -> Result<Value, SyntaxError> {
    let name = reader.word();
    CONTAINER_TYPES
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, container)| Value::Container(*container))
        .ok_or_else(|| reader.error("Expected barrel, furnace, or hopper"))
}

pub fn parse_player(reader: &mut Reader<'_>) -> Result<Value, SyntaxError> {
    let token = reader.token()?;
    if token == "@s" {
        return Ok(Value::Player(PlayerTarget::SelfPlayer));
    }
    if token.starts_with('@') {
        return Err(reader.error("Only @s, player names, and player UUIDs are supported"));
    }
    if token.len() == 36
        && token.bytes().enumerate().all(|(i, ch)| {
            if [8, 13, 18, 23].contains(&i) {
                ch == b'-'
            } else {
                ch.is_ascii_hexdigit()
            }
        })
        && let Ok(uuid) = u128::from_str_radix(&token.replace('-', ""), 16)
    {
        return Ok(Value::Player(PlayerTarget::Uuid(uuid)));
    }
    if token.is_empty()
        || token.len() > 16
        || !token
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == b'_')
    {
        return Err(reader.error("Expected a player name, UUID, or @s"));
    }
    Ok(Value::Player(PlayerTarget::Name(token.to_owned())))
}

pub fn parse_pattern(reader: &mut Reader<'_>) -> Result<Value, SyntaxError> {
    let text = reader.custom_string()?;
    WorldEditPattern::from_str(&text)
        .map(Value::Pattern)
        .map_err(|error| reader.error(error.to_string()))
}

pub fn parse_mask(reader: &mut Reader<'_>) -> Result<Value, SyntaxError> {
    let text = reader.custom_string()?;
    WorldEditMask::from_str(&text)
        .map(Value::Mask)
        .map_err(|error| reader.error(error.to_string()))
}

pub fn parse_replacement(reader: &mut Reader<'_>) -> Result<Value, SyntaxError> {
    let text = reader.custom_string()?;
    let pattern = WorldEditPattern::from_str(&text);
    let mask = WorldEditMask::from_str(&text);
    if let (Err(pattern), Err(mask)) = (&pattern, &mask) {
        let message = if text.starts_with(['!', '<', '>', '%', '#']) {
            mask.to_string()
        } else {
            pattern.to_string()
        };
        return Err(reader.error(message));
    }
    Ok(Value::Replacement(ReplacementOperand {
        pattern: pattern.ok(),
        mask: mask.ok(),
    }))
}

pub fn parse_direction(reader: &mut Reader<'_>) -> Result<Value, SyntaxError> {
    parse_direction_name(reader, DIRECTIONS).map(Value::Direction)
}

pub fn parse_direction_with_diagonals(reader: &mut Reader<'_>) -> Result<Value, SyntaxError> {
    parse_direction_name(reader, DIRECTIONS_WITH_DIAGONALS).map(Value::DirectionWithDiagonals)
}

fn parse_direction_name<T: Copy>(
    reader: &mut Reader<'_>,
    directions: &[(&[&str], T)],
) -> Result<T, SyntaxError> {
    let name = reader.word();
    directions
        .iter()
        .find(|(aliases, _)| {
            aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(&name))
        })
        .map(|(_, direction)| *direction)
        .ok_or_else(|| reader.error("Expected a direction"))
}
