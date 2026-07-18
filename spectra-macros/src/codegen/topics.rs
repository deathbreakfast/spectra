use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::dsl_parser::{EventSchemaSpec, MetricSchemaSpec};

use super::types::field_def_tokens;
use super::util::to_shouty_snake;

/// Topic constant + transport payload DTO for an event schema.
pub fn event_topic(spec: &EventSchemaSpec) -> TokenStream {
    let payload_name = format_ident!("{}Payload", spec.schema_name);
    let topic_const = format_ident!("{}_TOPIC", to_shouty_snake(&spec.schema_name));
    let table_lit = &spec.table;
    let topic_expr = format!("spectra.event.{table_lit}");

    let field_defs: Vec<_> = spec
        .fields
        .iter()
        .map(|f| field_def_tokens(&f.name, &f.rust_type))
        .collect();

    let json_fields: Vec<_> = spec
        .fields
        .iter()
        .map(|f| {
            let key = &f.name;
            let ident = format_ident!("{}", f.name);
            quote! { map.insert(#key.to_string(), serde_json::json!(self.#ident)); }
        })
        .collect();

    let payload_doc = format!("Transport payload for event table `{}`.", spec.table);
    let topic_doc = format!("Topic string for event table `{}`.", spec.table);

    quote! {
        #[doc = #topic_doc]
        pub const #topic_const: &str = #topic_expr;

        #[doc = #payload_doc]
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        pub struct #payload_name {
            /// Logical event table name.
            pub table: &'static str,
            #(#field_defs,)*
            /// Optional explicit emit timestamp.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub ts: Option<chrono::DateTime<chrono::Utc>>,
        }

        impl #payload_name {
            /// Stable transport topic for this event schema.
            pub fn topic() -> &'static str {
                #topic_const
            }

            /// Convert to a [`SpectraEvent`](::spectra::SpectraEvent) for publish adapters.
            pub fn to_spectra_event(&self) -> ::spectra::SpectraEvent {
                let mut map = serde_json::Map::new();
                #(#json_fields)*
                let fields = serde_json::Value::Object(map);
                match self.ts {
                    Some(ts) => ::spectra::SpectraEvent::with_ts(#table_lit, fields, ts),
                    None => ::spectra::SpectraEvent::new(#table_lit, fields),
                }
            }
        }
    }
}

/// Topic constant + transport payload DTO for a metric schema.
pub fn metric_topic(spec: &MetricSchemaSpec) -> TokenStream {
    let payload_name = format_ident!("{}Payload", spec.schema_name);
    let topic_const = format_ident!("{}_TOPIC", to_shouty_snake(&spec.schema_name));
    let name_lit = &spec.name;
    let topic_expr = format!("spectra.metric.{name_lit}");
    let payload_doc = format!("Transport payload for metric `{}`.", spec.name);
    let topic_doc = format!("Topic string for metric `{}`.", spec.name);

    quote! {
        #[doc = #topic_doc]
        pub const #topic_const: &str = #topic_expr;

        #[doc = #payload_doc]
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        pub struct #payload_name {
            /// Metric name.
            pub name: &'static str,
            /// JSON label object.
            pub labels: serde_json::Value,
            /// Counter delta.
            pub delta: i64,
            /// Optional explicit emit timestamp.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub ts: Option<chrono::DateTime<chrono::Utc>>,
        }

        impl #payload_name {
            /// Stable transport topic for this metric schema.
            pub fn topic() -> &'static str {
                #topic_const
            }

            /// Convert to a [`MetricEmit`](::spectra::MetricEmit) for publish adapters.
            pub fn to_metric_emit(&self) -> ::spectra::MetricEmit {
                match self.ts {
                    Some(ts) => ::spectra::MetricEmit::counter(
                        #name_lit,
                        self.labels.clone(),
                        self.delta,
                        ts,
                    ),
                    None => ::spectra::MetricEmit::counter(
                        #name_lit,
                        self.labels.clone(),
                        self.delta,
                        chrono::Utc::now(),
                    ),
                }
            }
        }
    }
}
