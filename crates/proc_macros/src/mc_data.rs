use nom::{
    bytes::complete::{take_till, take_until},
    character::complete::char,
    multi::separated_list1,
    sequence::{delimited, separated_pair},
    IResult, Parser,
};
use proc_macro::TokenStream;
use quote::quote;
use rustc_hash::FxHashMap;
use serde::Deserialize;
use std::{collections::HashMap, path::PathBuf, sync::LazyLock};
use syn::{Error, LitStr};

#[derive(Deserialize)]
struct BlockData {
    // properties: FxHashMap<String, serde_json::Value>,
    states: Vec<BlockState>,
}

#[derive(Deserialize)]
struct BlockState {
    id: u32,
    properties: Option<FxHashMap<String, serde_json::Value>>,
}

fn mc_data_path(name: &str) -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    PathBuf::from(manifest_dir).join("../../mc_data").join(name)
}

static BLOCKS: LazyLock<FxHashMap<String, BlockData>> = LazyLock::new(|| {
    let path = std::fs::read_to_string(mc_data_path("blocks.json")).unwrap();
    serde_json::from_str(&path).unwrap()
});

#[derive(Deserialize)]
struct RegistryEntry {
    protocol_id: i32,
}

#[derive(Deserialize)]
struct Registry {
    entries: FxHashMap<String, RegistryEntry>,
}

static REGISTRY: LazyLock<FxHashMap<String, Registry>> = LazyLock::new(|| {
    let path = std::fs::read_to_string(mc_data_path("registries.json")).unwrap();
    serde_json::from_str(&path).unwrap()
});

fn parse_props(input: &str) -> IResult<&str, HashMap<&str, &str>> {
    let (input, items) = separated_list1(
        char(','),
        separated_pair(
            take_until("="),
            char('='),
            take_till(|c| c == ']' || c == ','),
        ),
    )
    .parse(input)?;
    Ok((input, items.into_iter().collect()))
}

struct BlockInfo<'a> {
    name: &'a str,
    props: HashMap<&'a str, &'a str>,
}

fn parse_block(input: &'_ str) -> IResult<&'_ str, BlockInfo<'_>> {
    let (input, block_name) = take_till(|c| c == '[')(input)?;
    if input.is_empty() {
        return Ok((
            input,
            BlockInfo {
                name: block_name,
                props: HashMap::new(),
            },
        ));
    }

    let (_, props) = delimited(char('['), parse_props, char(']')).parse(input)?;
    let block_info = BlockInfo {
        name: block_name,
        props,
    };

    Ok((input, block_info))
}

pub fn get_block_id(str: LitStr) -> Result<TokenStream, Error> {
    let full_name = str.value();
    let (_, block_info) = parse_block(&full_name)
        .map_err(|_| Error::new_spanned(&str, "failed to parse block name"))?;

    let block = BLOCKS
        .get(block_info.name)
        .ok_or_else(|| Error::new_spanned(&str, "could not find block with name"))?;

    'states: for state in &block.states {
        if let Some(properties) = &state.properties {
            for (&name, &value) in &block_info.props {
                let prop = properties
                    .get(name)
                    .ok_or_else(|| Error::new_spanned(&str, "invalid property name"))?
                    .as_str()
                    .unwrap();
                if prop != value {
                    continue 'states;
                }
            }
        }
        let id = state.id;
        let tokens = quote! { #id };
        return Ok(TokenStream::from(tokens));
    }

    Err(Error::new_spanned(
        str,
        "could not find matching block state",
    ))
}

pub fn get_protocol_id(registry: LitStr, entry: LitStr) -> Result<TokenStream, Error> {
    let registry = REGISTRY
        .get(&registry.value())
        .ok_or_else(|| Error::new_spanned(registry, "invalid registry indentifier"))?;
    let entry = registry
        .entries
        .get(&entry.value())
        .ok_or_else(|| Error::new_spanned(entry, "invalid entry identifier"))?;

    let lit = entry.protocol_id;
    Ok(TokenStream::from(quote! { #lit }))
}
