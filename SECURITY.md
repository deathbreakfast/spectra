# Security policy

## Supported versions

Spectra is published as git tags (for example `v0.1.0`). Security fixes land on the default branch and are released in subsequent tags.

## Reporting a vulnerability

Please report security issues privately via GitHub Security Advisories for this repository:

https://github.com/unified-field-dev/spectra/security/advisories/new

Include a clear description, impact assessment, and reproduction steps when possible. Do not open a public issue for unfixed vulnerabilities.

We aim to acknowledge reports within a few business days.

## Operator hardening (L0)

Spectra is an **in-process** observability library. It does **not** implement session authentication, Gauge permissions, or HTTP ACL. Hosts own query authorization and network exposure.

| Area | Guidance |
|------|----------|
| Emit gate | `SPECTRA_GATE=0` is ignored unless `SPECTRA_GATE_FORCE_OFF=1` is also set (fail-closed). |
| Query identifiers | Metric/table/field tokens must match `validate_spectra_ident` before SQL runs. |
| Event paging | Event query `limit` / `offset` are clamped (`MAX_EVENT_QUERY_LIMIT` / `MAX_EVENT_QUERY_OFFSET`). |
| Remote TLS | Prefer `https://` or `tcp+tls://`. Plaintext `http://` / `tcp://` require `SPECTRA_ALLOW_INSECURE_REMOTE=1` (dev/CI only; emits a warning). |
| Connection secrets | Remote error mapping redacts URL userinfo; do not log raw credential URLs. |
| PII display | Call `mask_field_value` before rendering fields classified as PII. |
| Query authz | Enforce host policy (for example `spectra.query.{table}`) before calling query APIs. |

Discoverable API docs: `cargo doc -p uf-spectra --open` → **Features** (validation, paging clamps, gate, remote TLS, classification).
