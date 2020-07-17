use proc_macro::{Span, TokenStream};
use quote::quote;
use syn::{parse_macro_input, Abi, ItemFn, LitStr, token::Extern};

#[proc_macro_attribute]
pub fn event_handler(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut item = parse_macro_input!(item as ItemFn);
    item.sig.abi = Some(Abi {
        extern_token: Extern {
            span: Span::call_site().into(),
        },
        name: Some(LitStr::new("C", Span::call_site().into()))
    });
    let result = quote! {
        #item
    };
    result.into()
}
