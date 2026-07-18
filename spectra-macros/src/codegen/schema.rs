use proc_macro::TokenStream;
use quote::quote;

use crate::dsl_parser::EventSchemaSpec;

use super::{helpers, inventory, topics};

/// Expand `spectra_schema!` into inventory + typed logger + topic DTO.
pub fn expand(input: TokenStream) -> TokenStream {
    let spec = match syn::parse::<EventSchemaSpec>(input) {
        Ok(s) => s,
        Err(e) => return e.to_compile_error().into(),
    };

    let inventory_tokens = inventory::event_inventory(&spec);
    let helper_tokens = helpers::event_helper(&spec);
    let topic_tokens = topics::event_topic(&spec);

    quote! {
        #inventory_tokens
        #helper_tokens
        #topic_tokens
    }
    .into()
}
