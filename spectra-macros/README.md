# spectra-macros

Proc macros for typed Spectra schemas and metrics.

## Audience

| Reader | Use this crate for |
|--------|-------------------|
| **App developers** | `spectra_schema!` and `spectra_metric!` in application modules |
| **Library maintainers** | Macro expansion and DSL syntax |

## Role

- `spectra_schema!` — structured event log definitions with field classification
- `spectra_metric!` — counter and gauge metric definitions
- Metric `record` / `record_at` labels: JSON strings as-is; numbers and bools stringified;
  `null` / arrays / objects skipped
- Each expansion emits inventory registration, typed `*Logger` / `*Recorder`, and transport
  `*Payload` / `*_TOPIC` DTOs
- Token-only proc macro crate (no runtime deps on other Spectra crates)
- Link schema modules with a normal `mod` list so `inventory` collects registrations

## Status

Shipped in tag `v0.1.0`.
