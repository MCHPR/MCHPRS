use proc_macro::TokenStream;
use syn::{parse::Parse, parse_macro_input, DeriveInput, LitStr, Token};

mod block_attribs;
mod mc_data;

#[proc_macro_derive(BlockProperty)]
pub fn derive_block_property(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match block_attribs::create_block_property_impl(input) {
        Ok(ts) => ts,
        Err(err) => err.to_compile_error().into(),
    }
}

#[proc_macro_derive(BlockTransform)]
pub fn derive_block_transform(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match block_attribs::create_block_transform_impl(input) {
        Ok(ts) => ts,
        Err(err) => err.to_compile_error().into(),
    }
}

#[proc_macro]
pub fn block_id(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as LitStr);

    match mc_data::get_block_id(input) {
        Ok(ts) => ts,
        Err(err) => err.to_compile_error().into(),
    }
}

struct GetProtocolIdInput {
    registry: LitStr,
    _comma: Token![,],
    entry: LitStr,
}

impl Parse for GetProtocolIdInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self {
            registry: input.parse()?,
            _comma: input.parse()?,
            entry: input.parse()?,
        })
    }
}

#[proc_macro]
pub fn protocol_id(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as GetProtocolIdInput);

    match mc_data::get_protocol_id(input.registry, input.entry) {
        Ok(ts) => ts,
        Err(err) => err.to_compile_error().into(),
    }
}

struct GetPacketIdInput {
    state: LitStr,
    _comma1: Token![,],
    bound_to: LitStr,
    _comma2: Token![,],
    identifier: LitStr,
}

impl Parse for GetPacketIdInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self {
            state: input.parse()?,
            _comma1: input.parse()?,
            bound_to: input.parse()?,
            _comma2: input.parse()?,
            identifier: input.parse()?,
        })
    }
}

#[proc_macro]
pub fn packet_id(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as GetPacketIdInput);

    match mc_data::get_packet_id(input.state, input.bound_to, input.identifier) {
        Ok(ts) => ts,
        Err(err) => err.to_compile_error().into(),
    }
}
