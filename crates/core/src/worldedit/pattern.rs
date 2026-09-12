//! WorldEdit pattern parsing and block picking.
//!
//! Supported syntax (see <https://worldedit.enginehub.org/en/latest/usage/general/patterns/>):
//! - `stone`, `minecraft:repeater[delay=2]`: a single block, optionally with block states
//! - `=5922`: an exact block state ID
//! - `stone,50%dirt`: random pattern with optional relative weights
//! - `^stone`, `^[lit=true]`, `^redstone_lamp[lit=true]`: type/state applying pattern, which keeps the
//!   properties of the existing block that are not overwritten

use super::{parse_block, parse_block_states, split_top_level_commas, validate_states, World};
use mchprs_blocks::blocks::Block;
use mchprs_blocks::BlockPos;
use mchprs_commands::SuggestionsBuilder;
use rand::RngExt;
use rustc_hash::FxHashMap;
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq)]
pub struct WorldEditPattern(PatternSelection);

#[derive(Clone, Debug, PartialEq)]
enum PatternSelection {
    Single(BlockPattern),
    Random {
        patterns: Vec<WeightedPattern>,
        weight_total: f32,
    },
}

#[derive(Clone, Debug, PartialEq)]
enum BlockPattern {
    Fixed(Block),
    TypeApplying {
        block: Option<Block>,
        states: FxHashMap<String, String>,
    },
}

#[derive(Clone, Debug, PartialEq)]
struct WeightedPattern {
    weight: f32,
    pattern: BlockPattern,
}

impl WorldEditPattern {
    pub(crate) fn suggest(builder: &mut SuggestionsBuilder<'_, '_>) {
        super::suggestions::suggest(builder, [], |input| {
            matches!(
                input.parse::<Self>(),
                Ok(_) | Err(PatternParseError::ZeroWeight)
            )
        });
    }

    pub fn pick(&self, world: &impl World, pos: BlockPos) -> Block {
        let pattern = match &self.0 {
            PatternSelection::Single(pattern) => pattern,
            PatternSelection::Random {
                patterns,
                weight_total,
            } => {
                let mut random = rand::rng().random_range(0.0..*weight_total);
                let selected = patterns.iter().find(|pattern| {
                    random -= pattern.weight;
                    random < 0.0
                });
                &selected.unwrap_or_else(|| patterns.last().unwrap()).pattern
            }
        };
        pattern.pick(world, pos)
    }
}

impl BlockPattern {
    fn pick(&self, world: &impl World, pos: BlockPos) -> Block {
        match self {
            Self::Fixed(block) => *block,
            Self::TypeApplying { block, states } => {
                let existing = world.get_block(pos);
                let mut block = block.unwrap_or(existing);
                let existing_props = existing.properties();
                let mut props: HashMap<&str, &str> = existing_props
                    .iter()
                    .map(|(k, v)| (*k, v.as_str()))
                    .collect();
                props.extend(states.iter().map(|(k, v)| (k.as_str(), v.as_str())));
                props.retain(|name, value| {
                    block
                        .property_values(name)
                        .is_some_and(|values| values.contains(value))
                });
                block.set_properties(props);
                block
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PatternParseError {
    #[error("Expected a pattern")]
    Empty,
    #[error("Invalid pattern weight: {0}")]
    InvalidWeight(String),
    #[error("Pattern weights must add up to more than zero")]
    ZeroWeight,
    #[error("Pattern weight total is too large")]
    WeightOverflow,
    #[error("Unsupported pattern: {0}")]
    Unsupported(String),
    #[error("{0}")]
    Syntax(String),
}

impl FromStr for WorldEditPattern {
    type Err = PatternParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = input.trim();
        let parts = split_top_level_commas(input);
        if parts.len() == 1 && !input.contains('%') {
            return Ok(Self(PatternSelection::Single(input.parse()?)));
        }

        let mut patterns = parts
            .into_iter()
            .map(parse_weighted_pattern)
            .collect::<Result<Vec<_>, _>>()?;
        let weight_total: f32 = patterns.iter().map(|pattern| pattern.weight).sum();
        if !weight_total.is_finite() {
            return Err(PatternParseError::WeightOverflow);
        }
        if weight_total <= 0.0 {
            return Err(PatternParseError::ZeroWeight);
        }
        patterns.retain(|pattern| pattern.weight > 0.0);
        Ok(Self(PatternSelection::Random {
            patterns,
            weight_total,
        }))
    }
}

impl FromStr for BlockPattern {
    type Err = PatternParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if input.is_empty() {
            return Err(PatternParseError::Empty);
        }

        if let Some(rest) = input.strip_prefix('^') {
            if let Some(states_str) = rest.strip_prefix('[') {
                let states_str = states_str.strip_suffix(']').ok_or_else(|| {
                    PatternParseError::Syntax("Unclosed bracket in block states".to_owned())
                })?;
                let states = parse_block_states(states_str).map_err(PatternParseError::Syntax)?;
                validate_states(None, &states).map_err(PatternParseError::Syntax)?;
                return Ok(Self::TypeApplying {
                    block: None,
                    states,
                });
            }
            let (block, states) = parse_block(rest).map_err(PatternParseError::Syntax)?;
            return Ok(Self::TypeApplying {
                block: Some(block),
                states,
            });
        }

        if input.starts_with('*') || input.starts_with('#') {
            return Err(PatternParseError::Unsupported(input.to_string()));
        }

        if input.starts_with('=') {
            return super::parse_raw_block(input)
                .map(Self::Fixed)
                .map_err(PatternParseError::Syntax);
        }

        let (mut block, states) = parse_block(input).map_err(PatternParseError::Syntax)?;
        if !states.is_empty() {
            block.set_properties(
                states
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect(),
            );
        }
        Ok(Self::Fixed(block))
    }
}

fn parse_weighted_pattern(input: &str) -> Result<WeightedPattern, PatternParseError> {
    let input = input.trim();
    let (weight, pattern_str) = match input.split_once('%') {
        Some((weight_str, rest)) => {
            let weight = weight_str
                .parse::<f32>()
                .ok()
                .filter(|weight| weight.is_finite() && *weight >= 0.0)
                .ok_or_else(|| PatternParseError::InvalidWeight(weight_str.to_string()))?;
            if rest.contains('%') {
                return Err(PatternParseError::InvalidWeight(input.to_string()));
            }
            (weight / 100.0, rest)
        }
        _ => (1.0, input),
    };
    Ok(WeightedPattern {
        weight,
        pattern: pattern_str.trim().parse()?,
    })
}
