//! Proc macros for Spectra event and metric schema DSL.
//!
//! Compile-time only: expands into `SchemaMetadata` registration (via `inventory`), typed
//! emit helpers (`*Logger` / `*Recorder`), and transport topic DTOs through the `spectra`
//! crate. Use via the `spectra` re-export or depend on this crate directly.
//!
//! # Defining schemas
//!
//! A metric schema declares a named measurement family. An event schema declares a typed
//! structured-log table. Put declarations in modules you link into the binary:
//!
//! ```text
//! src/schemas/
//! ├── mod.rs          // pub mod cache_hits; pub mod request_log;
//! ├── cache_hits.rs
//! └── request_log.rs
//! ```
//!
//! The schema identifier determines the generated helper name:
//!
//! | Declaration | Generated helper | Storage/query key |
//! |-------------|------------------|-------------------|
//! | `CacheHits` | `CacheHitsRecorder` | metric `name` (`cache_hits`) |
//! | `RequestLog` | `RequestLogLogger` | event `table` (`request_log`) |
//!
//! Each `spectra_schema!` / `spectra_metric!` expansion also emits a `*Payload` transport DTO
//! and `*_TOPIC` constant for Mode 2 publish adapters.
//!
//! # Schema DSL features
//!
//! Declared on the schema (defaults applied at emit time; overridable at runtime via
//! `SpectraConfig` / `SPECTRA_LEVEL`, `SPECTRA_SAMPLE_*`, TOML):
//!
//! - **`level`** — verbosity tier (`Error` / `Warn` / `Info` / `Debug` / `Trace`; default `Info`)
//! - **`default_sample_rate`** — keep probability `0.0`–`1.0` (default `1.0`)
//! - **`coalesce_ms`** — gauge coalescing window (metrics only)
//! - **`classification`** — per-field `pii` / `safe_for_console` (events only)
//!
//! See [`spectra_schema!`] and [`spectra_metric!`] for full field tables and examples.
//!
//! Link schema modules so `inventory` collects registrations:
//!
//! ```ignore
//! // src/schemas/mod.rs
//! pub mod cache_hits;
//! pub mod request_log;
//! ```

mod codegen;
mod dsl_parser;

use proc_macro::TokenStream;

/// Declare a structured event log schema.
///
/// Expands to inventory registration, a typed payload struct, `*Logger`, and a transport
/// `*Payload` / `*_TOPIC` pair.
///
/// # DSL fields
///
/// | Field | Required | Description |
/// |-------|----------|-------------|
/// | `store` | yes | Logical store name for routing |
/// | `table` | yes | Event table name (registry key) |
/// | `version` | yes | Schema version string |
/// | `description` | no | Human-readable summary |
/// | `level` | no | Emit verbosity: `Error`, `Warn`, `Info` (default), `Debug`, or `Trace` |
/// | `default_sample_rate` | no | Keep probability `0.0`–`1.0` (default `1.0`); multiplied by the global sample rate |
/// | `fields` | yes | Column definitions with `r#type` and `classification` |
///
/// `coalesce_ms` is **not** valid on events (compile error); use it only on [`spectra_metric!`].
///
/// Supported helper field types are `String`, `i64`, `f64`, and `bool`.
///
/// Each field classification contains:
///
/// | Field | Meaning |
/// |-------|---------|
/// | `pii` | Marks personally identifiable data for downstream policy |
/// | `safe_for_console` | Allows the field in opt-in console/NDJSON mirroring |
///
/// Runtime overrides (`SPECTRA_LEVEL`, `SPECTRA_SAMPLE_RATE`, `SPECTRA_SAMPLE_<NAME>`, TOML)
/// merge on top of these schema defaults via `SpectraConfig`.
///
/// # Examples
///
/// ```ignore
/// use spectra::spectra_schema;
///
/// spectra_schema! {
///     RequestLog {
///         store: "default",
///         table: "request_log",
///         version: "0.1.0",
///         description: "Structured request events",
///         level: Debug,
///         default_sample_rate: 0.25,
///         fields: [
///             message: {
///                 r#type: String,
///                 classification: { pii: false, safe_for_console: true },
///             },
///             duration_ms: {
///                 r#type: i64,
///                 classification: { pii: false, safe_for_console: true },
///             },
///             user_id: {
///                 r#type: String,
///                 classification: { pii: true, safe_for_console: false },
///             },
///         ],
///     }
/// }
///
/// // RequestLogLogger::log(…); RequestLogPayload / REQUEST_LOG_TOPIC for transport
/// ```
#[proc_macro]
pub fn spectra_schema(input: TokenStream) -> TokenStream {
    codegen::schema::expand(input)
}

/// Declare a metric family schema.
///
/// Expands to inventory registration, `*Recorder`, and a transport `*Payload` / `*_TOPIC` pair.
///
/// # DSL fields
///
/// | Field | Required | Description |
/// |-------|----------|-------------|
/// | `store` | yes | Logical store name for routing |
/// | `name` | yes | Metric family name (registry key) |
/// | `version` | yes | Schema version string |
/// | `description` | no | Human-readable summary |
/// | `level` | no | Emit verbosity: `Error`, `Warn`, `Info` (default), `Debug`, or `Trace` |
/// | `default_sample_rate` | no | Keep probability `0.0`–`1.0` (default `1.0`); multiplied by the global sample rate |
/// | `coalesce_ms` | no | Gauge coalescing window in milliseconds (ignored for plain counters) |
///
/// Runtime overrides (`SPECTRA_LEVEL`, `SPECTRA_SAMPLE_RATE`, `SPECTRA_SAMPLE_<NAME>`, TOML)
/// merge on top of these schema defaults via `SpectraConfig`.
///
/// # Examples
///
/// ```ignore
/// use spectra::spectra_metric;
///
/// // Sampled debug counter — most emits dropped before the sink/persist path.
/// spectra_metric! {
///     CacheHits {
///         store: "default",
///         name: "cache_hits",
///         version: "0.1.0",
///         description: "Counter for cache hit events",
///         level: Debug,
///         default_sample_rate: 0.1,
///     }
/// }
///
/// // Gauge with coalescing — rapid updates within the window may merge.
/// spectra_metric! {
///     QueueDepth {
///         store: "default",
///         name: "queue_depth",
///         version: "0.1.0",
///         description: "In-flight queue depth",
///         level: Info,
///         coalesce_ms: 50,
///     }
/// }
///
/// // CacheHitsRecorder::record(delta, labels); CacheHitsPayload for transport
/// ```
#[proc_macro]
pub fn spectra_metric(input: TokenStream) -> TokenStream {
    codegen::metric::expand(input)
}
