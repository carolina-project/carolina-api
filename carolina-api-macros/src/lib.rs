use proc_macro::TokenStream;
use syn::{parse_macro_input, punctuated::Punctuated, ItemMod, Meta};

mod plugin;

/// Generate plugin api macros for the trait in the module.
///
/// Imported types in the module will be provided when using macros to generate static dispatching
/// enum, and the trait will be exported.
#[proc_macro_attribute]
pub fn plugin_api(attr: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemMod);
    let attr = parse_macro_input!(attr with Punctuated::<Meta, syn::Token![,]>::parse_terminated);

    plugin::api::parse_plugin_mod(attr.into_iter().collect(), input)
        .unwrap_or_else(|r| r.to_compile_error())
        .into()
}

#[doc(hidden)]
#[proc_macro]
pub fn __generate_enum(input: TokenStream) -> TokenStream {
    plugin::api::generate_enum(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
