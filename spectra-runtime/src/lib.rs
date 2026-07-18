//! Runtime assembly: builder, async storage persist, and optional off-thread telemetry I/O.
//!
//! Application crates usually depend on `spectra` instead of this crate directly.
//!
//! - [`Spectra::builder()`] / [`SpectraBuilder`] — inject backends, optional transport sink, and
//!   topology flags
//! - [`SpectraBuilder::persist`] — L2 queue/batch settings ([`PersistConfig`]; no env knobs)
//! - [`Spectra::flush_persist`] — durable barrier after `*_now` emits (Write Now scripts)
//! - [`SpectraBuilder::sink`] + [`SpectraBuilder::persist_disabled`] — **publisher** wiring for
//!   distributed ingest (consumers own storage; see `spectra` **Getting started → Mode 2**)
//! - [`StoragePersistSink`] — default async persist to registered storage backends
//! - [`OffThreadSpectraSink`] — off-thread NDJSON + console mirror (`telemetry-console` feature)
//! - Both `metrics_backend` and `events_backend` are required before [`SpectraBuilder::build`].
//! - With `persist_disabled`, a transport [`.sink`](SpectraBuilder::sink) is mandatory.

mod async_writer;
mod builder;
mod persist_config;
mod persist_sink;

pub use async_writer::{
    console_mirror_enabled, format_console_line, off_thread_emit_enabled, OffThreadSpectraSink,
};
pub use builder::{Spectra, SpectraBuilder};
pub use persist_config::{PersistConfig, PersistOverflow};
pub use persist_sink::{PersistHandle, StoragePersistSink};
