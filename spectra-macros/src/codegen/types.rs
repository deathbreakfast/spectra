use proc_macro2::TokenStream;
use quote::{format_ident, quote};

/// Map a DSL field type name to a Rust type token stream.
pub fn map_rust_type(rust_type: &str) -> TokenStream {
    match rust_type {
        "String" => quote! { String },
        "i64" => quote! { i64 },
        "f64" => quote! { f64 },
        "bool" => quote! { bool },
        _ => quote! { String },
    }
}

/// Field definition tokens for a public struct member.
pub fn field_def_tokens(name: &str, rust_type: &str) -> TokenStream {
    let ident = format_ident!("{}", name);
    let ty = map_rust_type(rust_type);
    let doc = format!("`{name}` event field.");
    quote! {
        #[doc = #doc]
        pub #ident: #ty
    }
}

/// Parameter tokens for a typed helper method.
pub fn field_param_tokens(name: &str, rust_type: &str) -> TokenStream {
    let ident = format_ident!("{}", name);
    let ty = map_rust_type(rust_type);
    quote! { #ident: #ty }
}
