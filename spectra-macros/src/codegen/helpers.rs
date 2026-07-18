use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::dsl_parser::{EventSchemaSpec, MetricSchemaSpec};

use super::types::{field_def_tokens, field_param_tokens};

/// Typed payload struct + `*Logger` for an event schema.
pub fn event_helper(spec: &EventSchemaSpec) -> TokenStream {
    let struct_name = format_ident!("{}", spec.schema_name);
    let logger_name = format_ident!("{}Logger", spec.schema_name);
    let table_lit = &spec.table;

    let field_defs: Vec<_> = spec
        .fields
        .iter()
        .map(|f| field_def_tokens(&f.name, &f.rust_type))
        .collect();

    let field_inits: Vec<_> = spec
        .fields
        .iter()
        .map(|f| {
            let ident = format_ident!("{}", f.name);
            quote! { #ident }
        })
        .collect();

    let field_params: Vec<_> = spec
        .fields
        .iter()
        .map(|f| field_param_tokens(&f.name, &f.rust_type))
        .collect();

    let json_fields: Vec<_> = spec
        .fields
        .iter()
        .map(|f| {
            let key = &f.name;
            let ident = format_ident!("{}", f.name);
            quote! { map.insert(#key.to_string(), serde_json::json!(#ident)); }
        })
        .collect();

    let struct_doc = format!("Event payload for table `{}`.", spec.table);
    let logger_doc = format!("Typed logger for event table `{}`.", spec.table);

    quote! {
        #[doc = #struct_doc]
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        pub struct #struct_name {
            #(#field_defs),*
        }

        #[doc = #logger_doc]
        pub struct #logger_name;

        impl #logger_name {
            /// Emit with an explicit timestamp (preserved via `current_emit_ts` at the sink).
            pub fn log_at(
                #(#field_params,)*
                ts: chrono::DateTime<chrono::Utc>,
            ) {
                let mut map = serde_json::Map::new();
                #(#json_fields)*
                let fields = serde_json::Value::Object(map);
                ::spectra::try_log_event_at(#table_lit, &fields, ts);
            }

            /// Emit with the current UTC timestamp.
            pub fn log(#(#field_params),*) {
                Self::log_at(#(#field_inits,)* chrono::Utc::now());
            }
        }
    }
}

/// Typed `*Recorder` for a metric schema.
pub fn metric_helper(spec: &MetricSchemaSpec) -> TokenStream {
    let helper_name = format_ident!("{}Recorder", spec.schema_name);
    let name_lit = &spec.name;
    let recorder_doc = format!("Typed recorder for metric `{}`.", spec.name);

    quote! {
        #[doc = #recorder_doc]
        pub struct #helper_name;

        impl #helper_name {
            /// Record with an explicit emit-time timestamp.
            ///
            /// Label values: JSON strings are used as-is; numbers and bools are stringified;
            /// `null`, arrays, and objects are skipped (not coerced to empty strings).
            pub fn record_at(
                delta: i64,
                labels: serde_json::Value,
                ts: chrono::DateTime<chrono::Utc>,
            ) {
                let owned: Vec<(String, String)> = labels
                    .as_object()
                    .map(|obj| {
                        obj.iter()
                            .filter_map(|(k, v)| {
                                let value = match v {
                                    serde_json::Value::String(s) => Some(s.clone()),
                                    serde_json::Value::Number(n) => Some(n.to_string()),
                                    serde_json::Value::Bool(b) => Some(b.to_string()),
                                    _ => None,
                                }?;
                                Some((k.clone(), value))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let label_pairs: Vec<(&str, &str)> = owned
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();
                ::spectra::try_record_counter_at(#name_lit, &label_pairs, delta, ts);
            }

            /// Record with the current UTC timestamp.
            pub fn record(delta: i64, labels: serde_json::Value) {
                Self::record_at(delta, labels, chrono::Utc::now());
            }
        }
    }
}
