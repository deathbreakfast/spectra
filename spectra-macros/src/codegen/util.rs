use quote::quote;
use syn::Ident;

use crate::dsl_parser::SpectraLevelSpec;

pub fn to_snake(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

pub fn to_shouty_snake(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.extend(c.to_uppercase());
        } else {
            out.push(c.to_ascii_uppercase());
        }
    }
    out
}

pub fn level_tokens(level: Option<SpectraLevelSpec>) -> proc_macro2::TokenStream {
    match level.unwrap_or(SpectraLevelSpec::Info) {
        SpectraLevelSpec::Error => quote! { ::spectra::SpectraLevel::Error },
        SpectraLevelSpec::Warn => quote! { ::spectra::SpectraLevel::Warn },
        SpectraLevelSpec::Info => quote! { ::spectra::SpectraLevel::Info },
        SpectraLevelSpec::Debug => quote! { ::spectra::SpectraLevel::Debug },
        SpectraLevelSpec::Trace => quote! { ::spectra::SpectraLevel::Trace },
    }
}

pub fn metadata_fn_ident(prefix: &str, schema_name: &str) -> Ident {
    Ident::new(
        &format!("{prefix}{}", to_snake(schema_name)),
        proc_macro2::Span::call_site(),
    )
}
