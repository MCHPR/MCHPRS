//! WorldEdit mask parsing and matching.
//!
//! Supported syntax (see <https://worldedit.enginehub.org/en/latest/usage/general/masks/>):
//! - `stone,repeater[delay=2]`: block type list, optionally with block states
//! - `=5922`: an exact block state ID, also allowed in block lists
//! - `!<mask>`: negation
//! - `>` / `<`: overlay / underlay (the block below / above must match)
//! - `%50`: random noise
//! - `#existing`, `#solid`, `#surface` / `#exposed`
//! - `^[state=value]` / `^=[state=value]`: block state mask (lenient / strict)
//! - Multiple masks separated by whitespace are intersected

use super::{parse_block, parse_block_states, split_top_level_commas, World};
use mchprs_blocks::blocks::Block;
use mchprs_blocks::{BlockFace, BlockPos};
use mchprs_commands::SuggestionsBuilder;
use rand::RngExt;
use rustc_hash::FxHashMap;
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq)]
pub struct WorldEditMask(Vec<MaskTerm>);

#[derive(Clone, Debug, PartialEq)]
struct MaskTerm {
    predicate: MaskPredicate,
    negated: bool,
    offset_y: i32,
}

#[derive(Clone, Debug, PartialEq)]
enum MaskPredicate {
    Blocks(Vec<BlockMatcher>),
    Existing,
    Solid,
    Surface,
    RandomNoise(f32),
    BlockStates {
        lenient: bool,
        states: FxHashMap<String, String>,
    },
}

const NAMED_PREDICATES: &[(&str, MaskPredicate)] = &[
    ("#existing", MaskPredicate::Existing),
    ("#solid", MaskPredicate::Solid),
    ("#surface", MaskPredicate::Surface),
    ("#exposed", MaskPredicate::Surface),
];

#[derive(Clone, Debug, PartialEq)]
enum BlockMatcher {
    Type {
        name: &'static str,
        states: FxHashMap<String, String>,
    },
    State(Block),
}

impl BlockMatcher {
    fn matches(&self, block: Block) -> bool {
        let Self::Type { name, states } = self else {
            return matches!(self, Self::State(expected) if *expected == block);
        };
        if block.get_name() != *name {
            return false;
        }
        if states.is_empty() {
            return true;
        }
        let props = block.properties();
        states.iter().all(|(k, v)| props.get(k.as_str()) == Some(v))
    }
}

impl WorldEditMask {
    pub(crate) fn suggest(builder: &mut SuggestionsBuilder<'_, '_>) {
        super::suggestions::suggest(
            builder,
            NAMED_PREDICATES.iter().map(|(name, _)| *name),
            |input| input.parse::<Self>().is_ok(),
        );
    }

    pub fn existing() -> Self {
        Self(vec![MaskTerm {
            predicate: MaskPredicate::Existing,
            negated: false,
            offset_y: 0,
        }])
    }

    pub fn matches(&self, world: &impl World, pos: BlockPos) -> bool {
        self.0.iter().all(|term| {
            let pos = BlockPos::new(pos.x, pos.y + term.offset_y, pos.z);
            term.predicate.matches(world, pos) != term.negated
        })
    }
}

impl MaskPredicate {
    fn matches(&self, world: &impl World, pos: BlockPos) -> bool {
        match self {
            Self::Blocks(matchers) => {
                let block = world.get_block(pos);
                matchers.iter().any(|matcher| matcher.matches(block))
            }
            Self::Existing => world.get_block(pos) != Block::Air,
            Self::Solid => is_movement_blocker(world.get_block(pos)),
            Self::Surface => {
                world.get_block(pos) != Block::Air
                    && BlockFace::values()
                        .into_iter()
                        .any(|face| world.get_block(pos.offset(face)) == Block::Air)
            }
            Self::RandomNoise(chance) => rand::rng().random::<f32>() < *chance,
            Self::BlockStates { lenient, states } => {
                let props = world.get_block(pos).properties();
                states.iter().all(|(k, v)| match props.get(k.as_str()) {
                    Some(bv) => bv == v,
                    None => *lenient,
                })
            }
        }
    }
}

// WorldEdit's #solid uses blocksMotion, which our block data doesn't store.
// Block::is_solid checks redstone conduction, so it cannot be reused here.
// Of the blocks we currently support, these have blocksMotion disabled by default in 1.20.4.
fn is_movement_blocker(block: Block) -> bool {
    !matches!(
        block,
        Block::Air
            | Block::RedstoneWire(_)
            | Block::Lever { .. }
            | Block::StoneButton { .. }
            | Block::RedstoneTorch { .. }
            | Block::RedstoneWallTorch { .. }
            | Block::Repeater(_)
            | Block::TripwireHook { .. }
            | Block::Comparator(_)
            | Block::SeaPickle { .. }
    )
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MaskParseError {
    #[error("Expected a mask")]
    Empty,
    #[error("Invalid percentage: {0}")]
    InvalidPercentage(String),
    #[error("Unsupported mask: {0}")]
    Unsupported(String),
    #[error("{0}")]
    Syntax(String),
}

impl FromStr for WorldEditMask {
    type Err = MaskParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let parts: Vec<_> = input.split_whitespace().collect();
        if parts.is_empty() {
            return Err(MaskParseError::Empty);
        }
        parts
            .into_iter()
            .map(MaskTerm::parse)
            .collect::<Result<_, _>>()
            .map(Self)
    }
}

impl MaskTerm {
    fn parse(mut input: &str) -> Result<Self, MaskParseError> {
        let mut negated = false;
        let mut offset_y = 0;
        loop {
            match input.as_bytes().first() {
                Some(b'!') => negated = !negated,
                Some(b'>') => offset_y -= 1,
                Some(b'<') => offset_y += 1,
                _ => break,
            }
            input = &input[1..];
        }

        Ok(Self {
            predicate: input.parse()?,
            negated,
            offset_y,
        })
    }
}

impl FromStr for MaskPredicate {
    type Err = MaskParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if input.is_empty() {
            return Err(MaskParseError::Empty);
        }

        if let Some(percent_str) = input.strip_prefix('%') {
            let percent = percent_str
                .parse::<f32>()
                .ok()
                .filter(|p| (0.0..=100.0).contains(p))
                .ok_or_else(|| MaskParseError::InvalidPercentage(input.to_string()))?;
            return Ok(Self::RandomNoise(percent / 100.0));
        }

        if let Some(rest) = input.strip_prefix('^') {
            let (lenient, states_str) = match rest.strip_prefix('=') {
                Some(strict) => (false, strict),
                None => (true, rest),
            };
            let states_str = states_str
                .strip_prefix('[')
                .and_then(|s| s.strip_suffix(']'))
                .ok_or_else(|| {
                    MaskParseError::Syntax("Expected [state=value] after ^".to_owned())
                })?;
            let states = parse_block_states(states_str).map_err(MaskParseError::Syntax)?;
            super::validate_states(None, &states).map_err(MaskParseError::Syntax)?;
            return Ok(Self::BlockStates { lenient, states });
        }

        if let Some((_, predicate)) = NAMED_PREDICATES.iter().find(|(name, _)| *name == input) {
            return Ok(predicate.clone());
        }
        if input.starts_with('#') || input.starts_with('$') {
            return Err(MaskParseError::Unsupported(input.to_string()));
        }

        parse_block_matchers(input)
    }
}

fn parse_block_matchers(input: &str) -> Result<MaskPredicate, MaskParseError> {
    let mut matchers = Vec::new();
    for block_str in split_top_level_commas(input) {
        let block_str = block_str.trim();
        if block_str.starts_with('=') {
            let block = super::parse_raw_block(block_str).map_err(MaskParseError::Syntax)?;
            matchers.push(BlockMatcher::State(block));
            continue;
        }
        let (block, states) = parse_block(block_str).map_err(MaskParseError::Syntax)?;
        matchers.push(BlockMatcher::Type {
            name: block.get_name(),
            states,
        });
    }
    Ok(MaskPredicate::Blocks(matchers))
}
