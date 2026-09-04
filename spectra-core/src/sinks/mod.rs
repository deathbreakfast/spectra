//! Built-in [`SpectraSink`](crate::SpectraSink) implementations for hosts and tests.

mod chained;
mod counting;
mod ndjson_file;
mod noop;
mod recording;

pub use chained::ChainedSink;
pub use counting::CountingSink;
pub use ndjson_file::NdjsonFileSink;
pub use noop::NoOpSink;
pub use recording::RecordingSink;
