use mchprs_blocks::blocks::{Block, PropertyDefinition};
use mchprs_commands::SuggestionsBuilder;
use rustc_hash::FxHashSet;
use std::iter;

pub(super) fn suggest(
    builder: &mut SuggestionsBuilder<'_, '_>,
    predicates: impl IntoIterator<Item = &'static str>,
    accepts: impl Fn(&str) -> bool,
) {
    let fragment = builder.remaining();
    if let Some(open) = fragment
        .rfind('[')
        .filter(|&open| !fragment[open..].contains(']'))
    {
        suggest_properties(builder, open, accepts);
        return;
    }

    let start = name_start(fragment);
    let prefix = &fragment[..start];
    let offset = builder.span().start + start;
    let mut name = builder.at(offset);
    if accepts(&format!("{prefix}stone")) {
        let namespaced = !name.remaining().is_empty();
        name.suggest_matching(Block::names().iter().flat_map(|name| {
            let short = name.strip_prefix("minecraft:").unwrap_or(name);
            iter::once(short).chain(namespaced.then_some(*name))
        }));
        if block_named(name.remaining()).is_some_and(|block| !block.properties().is_empty()) {
            name.suggest(format!("{}[", name.remaining()));
        }
    }
    name.suggest_matching(
        predicates
            .into_iter()
            .filter(|predicate| accepts(&format!("{prefix}{predicate}"))),
    );
}

fn suggest_properties(
    builder: &mut SuggestionsBuilder<'_, '_>,
    open: usize,
    accepts: impl Fn(&str) -> bool,
) {
    let fragment = builder.remaining();
    let properties = candidate_properties(&fragment[name_start(&fragment[..open])..open]);
    let property_start = fragment[open + 1..]
        .rfind(',')
        .map_or(open + 1, |comma| open + comma + 2);
    let used: FxHashSet<_> = fragment[open + 1..property_start]
        .split(',')
        .filter_map(|property| property.split_once('=').map(|(name, _)| name))
        .collect();
    if let Some((name, _)) = fragment[property_start..].split_once('=') {
        if used.contains(name) {
            return;
        }
        let Some(property) = properties.iter().find(|property| property.name == name) else {
            return;
        };
        let value_start = property_start + name.len() + 1;
        let prefix = &fragment[..value_start];
        let offset = builder.span().start + value_start;
        let mut value_builder = builder.at(offset);
        let has_more = properties
            .iter()
            .any(|other| other.name != name && !used.contains(other.name));
        for value in property.values {
            if !accepts(&format!("{prefix}{value}]")) {
                continue;
            }
            value_builder.suggest_matching([format!("{value}]")]);
            if has_more {
                value_builder.suggest_matching([format!("{value},")]);
            }
        }
    } else {
        let prefix = &fragment[..property_start];
        let offset = builder.span().start + property_start;
        let mut property_builder = builder.at(offset);
        property_builder.suggest_matching(
            properties
                .iter()
                .filter(|property| {
                    !used.contains(property.name)
                        && accepts(&format!(
                            "{prefix}{}={}]",
                            property.name, property.values[0]
                        ))
                })
                .map(|property| format!("{}=", property.name)),
        );
    }
}

fn candidate_properties(block_name: &str) -> Vec<PropertyDefinition> {
    match block_named(block_name) {
        Some(block) => block
            .properties()
            .into_keys()
            .filter_map(|name| {
                let values = block.property_values(name)?;
                Some(PropertyDefinition { name, values })
            })
            .collect(),
        None => Block::known_properties().to_vec(),
    }
}

fn name_start(input: &str) -> usize {
    input
        .char_indices()
        .rev()
        .find(|(_, character)| {
            !character.is_ascii_alphanumeric()
                && !matches!(character, '_' | '-' | '.' | '/' | ':' | '#')
        })
        .map_or(0, |(index, character)| index + character.len_utf8())
}

fn block_named(name: &str) -> Option<Block> {
    if name.contains(':') {
        Block::from_name(name)
    } else {
        Block::from_name(&format!("minecraft:{name}"))
    }
}
