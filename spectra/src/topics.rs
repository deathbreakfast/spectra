//! Transport topic constants and payload DTOs from this crate's CI smoke schemas.
//!
//! Applications get the same shapes from each `spectra_schema!` / `spectra_metric!` expansion
//! (`*Payload`, `*_TOPIC`) in the schema module.
//!
//! # Examples
//!
//! ```
//! use spectra::topics::{PlatformSmokeCounterPayload, PLATFORM_SMOKE_COUNTER_TOPIC};
//!
//! let payload = PlatformSmokeCounterPayload {
//!     name: "platform_smoke_counter",
//!     labels: serde_json::json!({"region": "us"}),
//!     delta: 1,
//!     ts: None,
//! };
//! assert_eq!(PlatformSmokeCounterPayload::topic(), PLATFORM_SMOKE_COUNTER_TOPIC);
//! let emit = payload.to_metric_emit();
//! assert_eq!(emit.name, "platform_smoke_counter");
//! ```

pub use crate::schemas::platform_smoke_counter::{
    PlatformSmokeCounterPayload, PLATFORM_SMOKE_COUNTER_TOPIC,
};
pub use crate::schemas::platform_smoke_event::{
    PlatformSmokeEventPayload, PLATFORM_SMOKE_EVENT_TOPIC,
};
