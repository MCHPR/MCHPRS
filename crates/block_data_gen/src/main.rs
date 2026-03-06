use convert_case::{Case, Casing};
use indexmap::IndexMap;
use proc_macro2::{Ident, Literal, Span, TokenStream};
use quote::{quote, ToTokens};
use serde::Deserialize;
use std::{collections::HashMap, fs, path::PathBuf, process::Command};

#[derive(Deserialize)]
struct BlockState {
    properties: Option<HashMap<String, String>>,
    id: u32,
    default: Option<bool>,
}

#[derive(Deserialize)]
struct BlockJson {
    properties: Option<IndexMap<String, Vec<String>>>,
    states: Vec<BlockState>,
}

#[derive(Deserialize)]
struct ItemRegistry {
    entries: HashMap<String, ItemRegistryEntry>,
}

#[derive(Deserialize)]
struct ItemRegistryEntry {
    protocol_id: u32,
}

#[derive(Deserialize)]
struct RegistriesJson {
    #[serde(rename = "minecraft:item")]
    items: ItemRegistry,
}

#[derive(Deserialize)]
struct InfoYaml {
    blocks: IndexMap<String, String>,
    items: IndexMap<String, String>,
    prop_types: IndexMap<String, Vec<String>>,
}

#[derive(Default, Debug)]
struct BlockAttrs {
    solid: bool,
    cube: bool,
    transparent: bool,
    maybe_solid: bool,
    maybe_cube: bool,
    maybe_transparent: bool,
    prop_struct: bool,
    item: bool,
    simple_item: bool,
    complex_transform: bool,
    wool: bool,
    wood: bool,
    stone: bool,
    glass: bool,
}

#[derive(Debug)]
struct Prop {
    name: String,
    ty: String,
    u8_offset: u8,
    default: String,
    num_values: usize,
}

#[derive(Debug)]
struct ProcessedBlock {
    name: String,
    pascal_name: String,
    props: Vec<Prop>,
    attrs: BlockAttrs,
    base_id: u32,
}

#[derive(Debug)]
struct ItemAttrs {
    max_stack: u32,
    block: bool,
    simple_placement: bool,
}

#[derive(Debug)]
struct ProcessedItem {
    name: String,
    pascal_name: String,
    id: u32,
    attrs: ItemAttrs,
}

impl ProcessedItem {
    fn new(name: &str, id: u32, attrs: ItemAttrs) -> Self {
        let (_, unnamespaced) = name.split_once(':').unwrap();
        Self {
            name: name.to_owned(),
            pascal_name: unnamespaced.to_case(Case::Pascal),
            id,
            attrs,
        }
    }
}

impl ProcessedBlock {
    fn match_ignore(&self) -> TokenStream {
        let ident = &ident(&self.pascal_name);
        let props = if self.attrs.prop_struct {
            Some(quote! { (_) })
        } else if !self.props.is_empty() {
            Some(quote! { { .. } })
        } else {
            None
        };
        quote! { Block::#ident #props }
    }

    fn match_props(&self) -> TokenStream {
        let props = self.props.iter().map(|prop| {
            let name = ident(&prop.name);
            quote! { #name }
        });
        let name_ident = &ident(&self.pascal_name);
        let props = if self.attrs.prop_struct {
            Some(quote! { (#name_ident { #( #props ),* }) })
        } else if !self.props.is_empty() {
            Some(quote! { { #( #props ),* } })
        } else {
            None
        };
        quote! { Block::#name_ident #props }
    }
}

fn process_block(
    name: String,
    attrs_str: String,
    blocks_json: &HashMap<String, BlockJson>,
    prop_types: &Vec<(String, Vec<String>)>,
) -> ProcessedBlock {
    let json = &blocks_json[&name];
    let default = json
        .states
        .iter()
        .find(|state| state.default == Some(true))
        .unwrap();
    let mut props = Vec::new();
    for (prop_name, prop_values) in json.properties.iter().flatten() {
        let default_props = default.properties.as_ref().unwrap();
        let prop_type = prop_types
            .iter()
            .find_map(|(name, values)| (values == prop_values).then(|| (name, values.len())));
        match prop_type {
            Some((prop_type, num_values)) => {
                props.push(Prop {
                    name: prop_name.clone(),
                    ty: prop_type.clone(),
                    default: default_props[prop_name].clone(),
                    num_values,
                    u8_offset: if prop_values[0] == "1" { 1 } else { 0 },
                });
            }
            None => panic!(
                "could not find prop type with values: {:?}, for block: {}",
                prop_values, name
            ),
        };
    }
    let mut attrs = BlockAttrs::default();
    for attr in attrs_str.split(',').filter(|s| !s.is_empty()) {
        match attr {
            "solid" => attrs.solid = true,
            "cube" => attrs.cube = true,
            "transparent" => attrs.transparent = true,
            "maybe_solid" => attrs.maybe_solid = true,
            "maybe_cube" => attrs.maybe_cube = true,
            "maybe_transparent" => attrs.maybe_transparent = true,
            "prop_struct" => attrs.prop_struct = true,
            "simple_item" => attrs.simple_item = true,
            "item" => attrs.item = true,
            "complex_transform" => attrs.complex_transform = true,
            "wool" => attrs.wool = true,
            "wood" => attrs.wood = true,
            "stone" => attrs.stone = true,
            "glass" => attrs.glass = true,
            _ => panic!("unknown block attribute: {}", attr),
        }
    }

    let (_, unnamespaced) = &name.split_once(':').unwrap();
    ProcessedBlock {
        pascal_name: unnamespaced.to_case(Case::Pascal),
        name,
        props,
        attrs,
        base_id: json.states[0].id,
    }
}

fn ident(str: &str) -> Ident {
    let str = match str {
        "type" => "ty",
        _ => str,
    };
    Ident::new(str, Span::call_site())
}

fn prop_literal(ty: &str, val: &str) -> TokenStream {
    match ty {
        "u8" => {
            let val = val.parse().unwrap();
            let lit = Literal::u8_unsuffixed(val);
            quote! { #lit }
        }
        "bool" => {
            // bools are idents for some reason
            let ident = ident(val);
            quote! { #ident }
        }
        _ => {
            let ty_ident = ident(ty);
            let variant = val.to_case(Case::Pascal);
            let variant = ident(&variant);
            quote! { #ty_ident::#variant }
        }
    }
}

fn generate_block_enum(blocks: &[ProcessedBlock]) -> TokenStream {
    let block_variants = blocks.iter().map(|block| {
        let name = &block.pascal_name;
        let name = Ident::new(&name, Span::call_site());
        let props = if block.attrs.prop_struct {
            Some(quote! { (#name) })
        } else if !block.props.is_empty() {
            let props = block.props.iter().map(|prop| {
                let name = ident(&prop.name);
                let ty = ident(&prop.ty);
                quote! { #name: #ty }
            });
            Some(quote! { { #( #props, )* } })
        } else {
            None
        };

        quote! {
            #name #props
        }
    });
    quote! {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Block {
            #(#block_variants,)*
        }
    }
}

fn generate_get_name(blocks: &[ProcessedBlock]) -> TokenStream {
    let match_arms = blocks.iter().map(|block| {
        let name = &block.name;
        let pat = block.match_ignore();
        quote! {
            #pat => #name
        }
    });

    quote! {
        pub fn get_name(self) -> &'static str {
            match self {
                #( #match_arms ),*
            }
        }
    }
}

fn generate_is_attr(
    blocks: &[ProcessedBlock],
    fn_name: &str,
    attr: fn(&ProcessedBlock) -> bool,
    dynamic_attr: Option<fn(&ProcessedBlock) -> bool>,
) -> TokenStream {
    let dynamic_fn = ident(&(fn_name.to_owned() + "_dynamic"));
    let match_arms = blocks
        .iter()
        .map(|block| {
            let is_dynamic = dynamic_attr.map_or(false, |f| f(block));
            if attr(block) || is_dynamic {
                let pat = block.match_ignore();
                Some(if is_dynamic {
                    quote! {
                        #pat => self.#dynamic_fn(),
                    }
                } else {
                    quote! {
                        #pat => true,
                    }
                })
            } else {
                None
            }
        })
        .flatten();
    let fn_ident = ident(fn_name);
    quote! {
        pub fn #fn_ident(self) -> bool {
            match self {
                #( #match_arms )*
                _ => false,
            }
        }
    }
}

fn generate_from_name(blocks: &[ProcessedBlock]) -> TokenStream {
    let match_arms = blocks.iter().map(|block| {
        let name = &block.name;
        let pascal_name = ident(&block.pascal_name);
        let init = block.props.iter().map(|prop| {
            let prop_name = ident(&prop.name);
            let literal = prop_literal(&prop.ty, &prop.default);
            quote! {
                #prop_name: #literal,
            }
        });

        let props = if block.attrs.prop_struct {
            Some(quote! {
                (#pascal_name { #( #init )* })
            })
        } else if !block.props.is_empty() {
            Some(quote! {
                { #( #init )* }
            })
        } else {
            None
        };

        quote! {
            #name => Block::#pascal_name #props,
        }
    });

    quote! {
        pub fn from_name(name: &str) -> Option<Block> {
            Some(match name {
                #( #match_arms )*
                _ => return None,
            })
        }
    }
}

fn generate_set_props(blocks: &[ProcessedBlock]) -> TokenStream {
    let match_arms = blocks.iter().map(|block| {
        let pat = block.match_props();
        let statements = block.props.iter().map(|prop| {
            let name = &prop.name;
            let ty_ident = ident(&prop.ty);
            let name_ident = ident(&prop.name);
            quote! {
                <#ty_ident as BlockProperty>::decode(#name_ident, &props, #name);
            }
        });
        quote! {
            #pat => { #( #statements )* }
        }
    });
    quote! {
        pub fn set_properties(&mut self, props: HashMap<&str, &str>) {
            match self {
                #( #match_arms )*
            }
        }
    }
}

fn generate_gen_props(blocks: &[ProcessedBlock]) -> TokenStream {
    let match_arms = blocks.iter().map(|block| {
        let pat = block.match_props();
        let statements = block.props.iter().map(|prop| {
            let name = &prop.name;
            let ty_ident = ident(&prop.ty);
            let name_ident = ident(&prop.name);
            quote! {
                <#ty_ident as BlockProperty>::encode(*#name_ident, &mut props, #name);
            }
        });
        quote! {
            #pat => { #( #statements )* }
        }
    });
    quote! {
        pub fn properties(&self) -> HashMap<&'static str, String> {
            let mut props = HashMap::new();
            match self {
                #( #match_arms )*
            }
            props
        }
    }
}

fn generate_rotate(blocks: &[ProcessedBlock]) -> TokenStream {
    let match_arms = blocks.iter().map(|block| {
        let pat = block.match_props();
        let statements = block.props.iter().map(|prop| {
            let ty_ident = ident(&prop.ty);
            let name_ident = ident(&prop.name);
            quote! {
                <#ty_ident as BlockTransform>::rotate(#name_ident, amt);
            }
        });
        if block.attrs.complex_transform {
            quote! {
                #pat => self.complex_rotate(amt),
            }
        } else {
            quote! {
                #pat => { #( #statements )* }
            }
        }
    });
    quote! {
        pub fn rotate(&mut self, amt: RotateAmt) {
            match self {
                #( #match_arms )*
            }
        }
    }
}

fn generate_flip(blocks: &[ProcessedBlock]) -> TokenStream {
    let match_arms = blocks.iter().map(|block| {
        let pat = block.match_props();
        let statements = block.props.iter().map(|prop| {
            let ty_ident = ident(&prop.ty);
            let name_ident = ident(&prop.name);
            quote! {
                <#ty_ident as BlockTransform>::flip(#name_ident, dir);
            }
        });
        if block.attrs.complex_transform {
            quote! {
                #pat => self.complex_flip(dir),
            }
        } else {
            quote! {
                #pat => { #( #statements )* }
            }
        }
    });
    quote! {
        pub fn flip(&mut self, dir: FlipDirection) {
            match self {
                #( #match_arms )*
            }
        }
    }
}

fn get_prop_id(name: &str, ty: &str, u8_offset: u8) -> TokenStream {
    let offset_lit = Literal::u8_unsuffixed(u8_offset);
    let name_ident = ident(name);
    match ty {
        "bool" => quote! { !#name_ident as u32 },
        "u8" => quote! { #name_ident as u32 - #offset_lit },
        _ => quote! { #name_ident.get_id() },
    }
}

fn from_prop_id(ty: &str, id: TokenStream, u8_offset: u8) -> TokenStream {
    let offset_lit = Literal::u8_unsuffixed(u8_offset);
    let ty_ident = ident(ty);
    match ty {
        "bool" => quote! { (#id & 1) == 0 },
        "u8" => quote! { #id as u8 + #offset_lit },
        _ => quote! { #ty_ident::from_id(#id) },
    }
}

fn generate_get_id(blocks: &[ProcessedBlock]) -> TokenStream {
    let match_arms = blocks.iter().map(|block| {
        let mut ts = TokenStream::new();
        ts.extend(Literal::u32_unsuffixed(block.base_id).into_token_stream());
        for (idx, prop) in block.props.iter().enumerate() {
            let mult = if idx + 1 < block.props.len() {
                let factor = block.props[idx + 1..]
                    .iter()
                    .fold(1, |a, b| a * b.num_values);
                let lit = Literal::usize_unsuffixed(factor);
                Some(quote! { * #lit })
            } else {
                None
            };
            let prop_id = get_prop_id(&prop.name, &prop.ty, prop.u8_offset);
            ts.extend(quote! { + (#prop_id) #mult });
        }
        let pat = block.match_props();
        quote! {
            #pat => #ts,
        }
    });
    quote! {
        pub fn get_id(self) -> u32 {
            match self {
                #( #match_arms )*
            }
        }
    }
}

fn generate_from_id(blocks: &[ProcessedBlock]) -> TokenStream {
    let match_arms = blocks.iter().map(|block| {
        let name_ident = ident(&block.pascal_name);
        let num_states = block.props.iter().fold(1, |a, b| a * b.num_values);
        let props = block.props.iter().enumerate().map(|(idx, prop)| {
            let prop_name_ident = ident(&prop.name);
            let div = if idx + 1 < block.props.len() {
                let div = block.props[idx + 1..]
                    .iter()
                    .fold(1, |a, b| a * b.num_values);
                let lit = Literal::usize_unsuffixed(div);
                Some(quote! { / #lit })
            } else {
                None
            };
            let modulo = Literal::usize_unsuffixed(prop.num_values);
            let val = from_prop_id(
                &prop.ty,
                quote! {
                    ((id #div) % #modulo)
                },
                prop.u8_offset,
            );
            quote! { #prop_name_ident: #val, }
        });
        let props = if block.attrs.prop_struct {
            Some(quote! { (#name_ident { #( #props )* })})
        } else if !block.props.is_empty() {
            Some(quote! { { #( #props )* } })
        } else {
            None
        };

        let min = Literal::u32_unsuffixed(block.base_id);
        let max = Literal::u32_unsuffixed(block.base_id + num_states as u32);
        quote! {
            #min..#max => {
                id -= #min;
                Block::#name_ident #props
            },
        }
    });
    quote! {
        pub fn from_id(mut id: u32) -> Block {
            match id {
                #( #match_arms )*
                _ => Block::Air,
            }
        }
    }
}

fn generate_prop_from_str_impls(prop_types: &Vec<(String, Vec<String>)>) -> TokenStream {
    let impls = prop_types
        .iter()
        .filter(|(name, _)| name != "u8" && name != "bool")
        .map(|(name, values)| {
            let prop_ident = ident(&name.to_case(Case::Pascal));
            let match_arms = values.iter().map(|value| {
                let value_ident = ident(&value.to_case(Case::Pascal));
                quote! { #value => #prop_ident::#value_ident, }
            });
            quote! {
                impl FromStr for #prop_ident {
                    type Err = ();

                    fn from_str(s: &str) -> Result<Self, Self::Err> {
                        Ok(match s {
                            #( #match_arms )*
                            _ => return Err(()),
                        })
                    }
                }
            }
        });
    quote! { #( #impls )* }
}

fn generate_prop_display_impls(prop_types: &Vec<(String, Vec<String>)>) -> TokenStream {
    let impls = prop_types
        .iter()
        .filter(|(name, _)| name != "u8" && name != "bool")
        .map(|(name, values)| {
            let prop_ident = ident(&name.to_case(Case::Pascal));
            let match_arms = values.iter().map(|value| {
                let value_ident = ident(&value.to_case(Case::Pascal));
                quote! { #prop_ident::#value_ident => #value, }
            });
            quote! {
                impl std::fmt::Display for #prop_ident {
                    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        f.write_str(match self {
                            #( #match_arms )*
                        })
                    }
                }
            }
        });
    quote! { #( #impls )* }
}

fn generate_prop_get_from_id(prop_types: &Vec<(String, Vec<String>)>) -> TokenStream {
    let impls = prop_types
        .iter()
        .filter(|(name, _)| name != "u8" && name != "bool")
        .map(|(name, values)| {
            let prop_ident = ident(&name.to_case(Case::Pascal));
            let match_arms = values.iter().enumerate().map(|(idx, value)| {
                let lit = Literal::usize_unsuffixed(idx);
                let value_ident = ident(&value.to_case(Case::Pascal));
                quote! { #lit => #prop_ident::#value_ident, }
            });
            quote! {
                impl #prop_ident {
                    fn get_id(self) -> u32 {
                        self as u32
                    }

                    fn from_id(id: u32) -> Self {
                        match id {
                            #( #match_arms )*
                            id => unreachable!(),
                        }
                    }
                }
            }
        });

    quote! { #( #impls )* }
}

fn generate_item_enum(items: &[ProcessedItem]) -> TokenStream {
    let variants = items.iter().map(|item| {
        let name_ident = ident(&item.pascal_name);
        quote! { #name_ident, }
    });
    quote! {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Item {
            #( #variants )*
            Unknown(u32)
        }
    }
}

fn generate_item_get_id(items: &[ProcessedItem]) -> TokenStream {
    let match_arms = items.iter().map(|item| {
        let name_ident = ident(&item.pascal_name);
        let lit = Literal::u32_unsuffixed(item.id);
        quote! {
            Item::#name_ident => #lit,
        }
    });
    quote! {
        pub fn get_id(self) -> u32 {
            match self {
                #( #match_arms )*
                Item::Unknown(id) => id,
            }
        }
    }
}

fn generate_item_from_id(items: &[ProcessedItem]) -> TokenStream {
    let match_arms = items.iter().map(|item| {
        let name_ident = ident(&item.pascal_name);
        let lit = Literal::u32_unsuffixed(item.id);
        quote! {
            #lit => Item::#name_ident,
        }
    });
    quote! {
        pub fn from_id(id: u32) -> Item {
            match id {
                #( #match_arms )*
                _ => Item::Unknown(id),
            }
        }
    }
}

fn generate_item_from_name(items: &[ProcessedItem]) -> TokenStream {
    let match_arms = items.iter().map(|item| {
        let name = &item.name;
        let name_ident = ident(&item.pascal_name);
        quote! {
            #name => Item::#name_ident,
        }
    });
    quote! {
        pub fn from_name(name: &str) -> Option<Item> {
            Some(match name {
                #( #match_arms )*
                _ => return None,
            })
        }
    }
}

fn generate_item_get_name(items: &[ProcessedItem]) -> TokenStream {
    let match_arms = items.iter().map(|item| {
        let name = &item.name;
        let name_ident = ident(&item.pascal_name);
        quote! {
            Item::#name_ident => #name,
        }
    });
    quote! {
        pub fn get_name(self) -> &'static str {
            match self {
                #( #match_arms )*
                Item::Unknown(_) => "minecraft:redstone",
            }
        }
    }
}

fn generate_item_simple_placements(items: &[ProcessedItem]) -> TokenStream {
    let match_arms = items
        .iter()
        .filter(|item| item.attrs.simple_placement)
        .map(|item| {
            let name_ident = ident(&item.pascal_name);
            quote! {
                Item::#name_ident => Some(Block::#name_ident),
            }
        });
    quote! {
        pub fn get_simple_placement(self) -> Option<Block> {
            match self {
                #( #match_arms )*
                _ => None,
            }
        }
    }
}

fn generate_item_is_block(items: &[ProcessedItem]) -> TokenStream {
    let match_arms = items.iter().map(|item| {
        let name_ident = ident(&item.pascal_name);
        let is_block = ident(&item.attrs.block.to_string());
        quote! {
            Item::#name_ident => #is_block,
        }
    });
    quote! {
        pub fn is_block(self) -> bool {
            match self {
                #( #match_arms )*
                Item::Unknown(_) => false,
            }
        }
    }
}

fn generate_item_max_stack_size(items: &[ProcessedItem]) -> TokenStream {
    let match_arms = items.iter().map(|item| {
        let name_ident = ident(&item.pascal_name);
        let lit = Literal::u32_unsuffixed(item.attrs.max_stack);
        quote! {
            Item::#name_ident => #lit,
        }
    });
    quote! {
        pub fn max_stack_size(self) -> u32 {
            match self {
                #( #match_arms )*
                Item::Unknown(_) => 64,
            }
        }
    }
}

fn generate_module(
    blocks: &[ProcessedBlock],
    prop_types: &Vec<(String, Vec<String>)>,
    items: &[ProcessedItem],
) -> TokenStream {
    let block_enum = generate_block_enum(blocks);

    let get_name = generate_get_name(blocks);
    let from_name = generate_from_name(blocks);
    let is_solid = generate_is_attr(
        blocks,
        "is_solid",
        |block| block.attrs.solid,
        Some(|block| block.attrs.maybe_solid),
    );
    let is_cube = generate_is_attr(
        blocks,
        "is_cube",
        |block| block.attrs.cube,
        Some(|block| block.attrs.maybe_cube),
    );
    let is_transparent = generate_is_attr(
        blocks,
        "is_transparent",
        |block| block.attrs.transparent,
        Some(|block| block.attrs.maybe_transparent),
    );
    let set_props = generate_set_props(blocks);
    let gen_props = generate_gen_props(blocks);
    let rotate = generate_rotate(blocks);
    let flip = generate_flip(blocks);
    let get_id = generate_get_id(blocks);
    let from_id = generate_from_id(blocks);
    let is_wood = generate_is_attr(blocks, "is_wood", |block| block.attrs.wood, None);
    let is_wool = generate_is_attr(blocks, "is_wool", |block| block.attrs.wool, None);
    let is_stone = generate_is_attr(blocks, "is_stone", |block| block.attrs.stone, None);
    let is_glass = generate_is_attr(blocks, "is_glass", |block| block.attrs.glass, None);

    let prop_from_str = generate_prop_from_str_impls(prop_types);
    let prop_display = generate_prop_display_impls(prop_types);
    let prop_get_from_id = generate_prop_get_from_id(prop_types);

    let item_enum = generate_item_enum(items);
    let item_get_id = generate_item_get_id(items);
    let item_from_id = generate_item_from_id(items);
    let item_get_name = generate_item_get_name(items);
    let item_from_name = generate_item_from_name(items);
    let item_simple_placements = generate_item_simple_placements(items);
    let item_is_block = generate_item_is_block(items);
    let item_max_stack_size = generate_item_max_stack_size(items);

    quote! {
        #![allow(unused_parens, unused_assignments, non_contiguous_range_endpoints, unused_variables)]

        use crate::{*, blocks::*};

        #block_enum

        impl Block {
            #get_name
            #from_name
            #is_solid
            #is_cube
            #is_transparent
            #set_props
            #gen_props
            #rotate
            #flip
            #get_id
            #from_id
            #is_wool
            #is_wood
            #is_stone
            #is_glass
        }

        #prop_from_str
        #prop_display
        #prop_get_from_id

        #item_enum

        impl Item {
            #item_get_id
            #item_from_id
            #item_get_name
            #item_from_name
            #item_simple_placements
            #item_is_block
            #item_max_stack_size
        }
    }
}

fn main() {
    let mc_data_path = PathBuf::from("../../mc_data");
    let output_path = PathBuf::from("../blocks/src/generated.rs");

    let yaml: InfoYaml =
        serde_yaml_ng::from_str(&fs::read_to_string(mc_data_path.join("gen_info.yaml")).unwrap())
            .unwrap();

    let blocks_json: HashMap<String, BlockJson> =
        serde_json::from_str(&fs::read_to_string(mc_data_path.join("blocks.json")).unwrap())
            .unwrap();
    let registries_json: RegistriesJson =
        serde_json::from_str(&fs::read_to_string(mc_data_path.join("registries.json")).unwrap())
            .unwrap();

    let mut prop_types = yaml.prop_types.into_iter().collect::<Vec<_>>();
    prop_types.push((
        "bool".to_owned(),
        vec!["true".to_owned(), "false".to_owned()],
    ));
    prop_types.push(("u8".to_owned(), (0..25).map(|n| n.to_string()).collect()));
    prop_types.push(("u8".to_owned(), (0..16).map(|n| n.to_string()).collect()));
    prop_types.push(("u8".to_owned(), (0..9).map(|n| n.to_string()).collect()));
    prop_types.push(("u8".to_owned(), (0..7).map(|n| n.to_string()).collect()));
    prop_types.push(("u8".to_owned(), (1..=4).map(|n| n.to_string()).collect()));
    prop_types.push(("u8".to_owned(), (1..=3).map(|n| n.to_string()).collect()));

    let processed_blocks = yaml
        .blocks
        .into_iter()
        .map(|(name, attrs)| process_block(name, attrs, &blocks_json, &prop_types))
        .collect::<Vec<_>>();

    let mut items = Vec::new();
    for block in &processed_blocks {
        if block.attrs.simple_item | block.attrs.item {
            let attrs = ItemAttrs {
                block: true,
                max_stack: 64,
                simple_placement: block.attrs.simple_item,
            };
            let id = registries_json
                .items
                .entries
                .get(&block.name)
                .expect(&block.name)
                .protocol_id;
            items.push(ProcessedItem::new(&block.name, id, attrs));
        }
    }

    for (name, attrs_str) in &yaml.items {
        let mut attrs = ItemAttrs {
            block: false,
            max_stack: 64,
            simple_placement: false,
        };
        attrs_str
            .split(',')
            .filter(|s| !s.is_empty())
            .for_each(|attr_str| {
                if let Some(num_str) = attr_str.strip_prefix("max_stack:") {
                    attrs.max_stack = num_str.parse().unwrap();
                } else if attr_str == "block" {
                    attrs.block = true;
                }
            });
        items.push(ProcessedItem::new(
            name,
            registries_json.items.entries[name].protocol_id,
            attrs,
        ));
    }

    let gen_src = generate_module(&processed_blocks, &prop_types, &items).to_string();
    fs::write(&output_path, gen_src).unwrap();
    Command::new("rustfmt")
        .arg(&output_path)
        .spawn()
        .expect("failed to run rustfmt");
}
