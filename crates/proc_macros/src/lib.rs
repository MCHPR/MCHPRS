use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};

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
    let input = parse_macro_input!(input as syn::LitStr);

    match mc_data::get_block_id(input) {
        Ok(ts) => ts,
        Err(err) => err.to_compile_error().into(),
    }
}