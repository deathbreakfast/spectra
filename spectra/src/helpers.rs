//! Typed emit helpers from this crate's CI smoke schemas.
//!
//! Applications get the same shapes by declaring `spectra_schema!` / `spectra_metric!` in
//! modules they `mod` into their crate — helpers expand at the declaration site.
//!
//! # Examples
//!
//! ```
//! use spectra::helpers::{PlatformSmokeCounterRecorder, PlatformSmokeEventLogger};
//!
//! PlatformSmokeCounterRecorder::record(1, serde_json::json!({"region": "us"}));
//! PlatformSmokeEventLogger::log("request handled".to_string());
//! ```

pub use crate::schemas::platform_smoke_counter::PlatformSmokeCounterRecorder;
pub use crate::schemas::platform_smoke_event::{PlatformSmokeEvent, PlatformSmokeEventLogger};
