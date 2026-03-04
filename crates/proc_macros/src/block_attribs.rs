use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Error, Ident, Type};

pub fn create_block_property_impl(input: DeriveInput) -> Result<TokenStream, Error> {
    let fields = match input.data {
        Data::Struct(ds) => ds.fields,
        _ => {
            return Err(Error::new_spanned(
                input,
                "BlockProperty proxy type must be a struct",
            ))
        }
    };
    let field_types: Vec<&Type> = fields.iter().map(|f| &f.ty).collect();
    let field_names: Vec<&Ident> = fields.iter().map(|f| f.ident.as_ref().unwrap()).collect();
    let struct_name = input.ident;

    let tokens = quote! {
        impl BlockProperty for #struct_name {
            fn encode(self, props: &mut ::std::collections::HashMap<&'static str, String>, _name: &'static str) {
                #(
                    <#field_types as BlockProperty>::encode(self.#field_names, props, stringify!(#field_names));
                )*
            }

            fn decode(&mut self, props: &::std::collections::HashMap<&str, &str>, _name: &str) {
                #(
                    <#field_types as BlockProperty>::decode(&mut self.#field_names, props, stringify!(#field_names));
                )*
            }
        }
    };
    Ok(tokens.into())
}

pub fn create_block_transform_impl(input: DeriveInput) -> Result<TokenStream, Error> {
    let fields = match input.data {
        Data::Struct(ds) => ds.fields,
        _ => {
            return Err(Error::new_spanned(
                input,
                "BlockTransform proxy type must be a struct",
            ))
        }
    };
    let field_types: Vec<&Type> = fields.iter().map(|f| &f.ty).collect();
    let field_names: Vec<&Ident> = fields.iter().map(|f| f.ident.as_ref().unwrap()).collect();
    let struct_name = input.ident;

    let tokens = quote! {
        impl crate::blocks::BlockTransform for #struct_name {
            fn rotate90(&mut self) {
                #(
                    <#field_types as crate::blocks::BlockTransform>::rotate90(&mut self.#field_names);
                )*
            }

            fn flip(&mut self, dir: crate::blocks::FlipDirection) {
                #(
                    <#field_types as crate::blocks::BlockTransform>::flip(&mut self.#field_names, dir);
                )*
            }

        }
    };
    Ok(tokens.into())
}
