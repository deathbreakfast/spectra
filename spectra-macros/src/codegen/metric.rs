use proc_macro::TokenStream;
use quote::quote;

use crate::dsl_parser::MetricSchemaSpec;

use super::{helpers, inventory, topics};

/// Expand `spectra_metric!` into inventory + typed recorder + topic DTO.
pub fn expand(input: TokenStream) -> TokenStream {
    let spec = match syn::parse::<MetricSchemaSpec>(input) {
        Ok(s) => s,
        Err(e) => return e.to_compile_error().into(),
    };

    let inventory_tokens = inventory::metric_inventory(&spec);
    let helper_tokens = helpers::metric_helper(&spec);
    let topic_tokens = topics::metric_topic(&spec);

    quote! {
        #inventory_tokens
        #helper_tokens
        #topic_tokens
    }
    .into()
}
