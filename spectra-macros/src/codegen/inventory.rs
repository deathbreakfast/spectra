use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::LitStr;

// quote! used throughout this module

use crate::dsl_parser::{EventSchemaSpec, MetricSchemaSpec};

use super::util::{level_tokens, metadata_fn_ident};

/// Inventory registration tokens for an event schema.
pub fn event_inventory(spec: &EventSchemaSpec) -> TokenStream {
    let table_lit = LitStr::new(&spec.table, Span::call_site());
    let version_lit = LitStr::new(&spec.version, Span::call_site());
    let fn_name = metadata_fn_ident("__spectra_schema_metadata_", &spec.schema_name);
    let description_tokens = optional_description(spec.description.as_ref());
    let level = level_tokens(spec.level);
    let sample_rate = spec.default_sample_rate.unwrap_or(1.0);
    let store_lit = LitStr::new(spec.store.as_deref().unwrap_or("default"), Span::call_site());

    let field_meta: Vec<_> = spec
        .fields
        .iter()
        .map(|f| {
            let name = &f.name;
            let rust_type = &f.rust_type;
            let pii = f.pii;
            let safe = f.safe_for_console;
            quote! {
                ::spectra::SchemaFieldMetadata {
                    name: #name.to_string(),
                    rust_type: #rust_type.to_string(),
                    classification: ::spectra::FieldClassification {
                        pii: #pii,
                        safe_for_console: #safe,
                        retention_days: None,
                        purpose: None,
                    },
                }
            }
        })
        .collect();

    quote! {
        fn #fn_name() -> ::spectra::SchemaMetadata {
            ::spectra::SchemaMetadata {
                table_or_metric: #table_lit.to_string(),
                store: #store_lit.to_string(),
                version: #version_lit.to_string(),
                description: #description_tokens,
                logging_kind: ::spectra::LoggingKind::Event,
                fields: vec![ #(#field_meta),* ],
                default_level: #level,
                default_sample_rate: #sample_rate,
                gauge_coalesce_ms: None,
            }
        }

        ::spectra::inventory::submit! {
            ::spectra::SchemaMetadataInit(#fn_name)
        }
    }
}

/// Inventory registration tokens for a metric schema.
pub fn metric_inventory(spec: &MetricSchemaSpec) -> TokenStream {
    let name_lit = LitStr::new(&spec.name, Span::call_site());
    let version_lit = LitStr::new(&spec.version, Span::call_site());
    let fn_name = metadata_fn_ident("__spectra_metric_metadata_", &spec.schema_name);
    let description_tokens = optional_description(spec.description.as_ref());
    let level = level_tokens(spec.level);
    let sample_rate = spec.default_sample_rate.unwrap_or(1.0);
    let store_lit = LitStr::new(spec.store.as_deref().unwrap_or("default"), Span::call_site());
    let coalesce_tokens = match spec.coalesce_ms {
        Some(ms) => quote! { Some(#ms) },
        None => quote! { None },
    };

    quote! {
        fn #fn_name() -> ::spectra::SchemaMetadata {
            ::spectra::SchemaMetadata {
                table_or_metric: #name_lit.to_string(),
                store: #store_lit.to_string(),
                version: #version_lit.to_string(),
                description: #description_tokens,
                logging_kind: ::spectra::LoggingKind::Metric,
                fields: vec![],
                default_level: #level,
                default_sample_rate: #sample_rate,
                gauge_coalesce_ms: #coalesce_tokens,
            }
        }

        ::spectra::inventory::submit! {
            ::spectra::SchemaMetadataInit(#fn_name)
        }
    }
}

fn optional_description(description: Option<&String>) -> TokenStream {
    match description {
        Some(d) => {
            let lit = LitStr::new(d, Span::call_site());
            quote! { Some(#lit.to_string()) }
        }
        None => quote! { None },
    }
}
