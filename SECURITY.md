# Security policy

## Supported versions

Spectra is published as git tags (for example `v0.1.0`). Security fixes land on the default branch and are released in subsequent tags.

## Reporting a vulnerability

Please report security issues privately via GitHub Security Advisories for this repository:

https://github.com/unified-field-dev/spectra/security/advisories/new

Include a clear description, impact assessment, and reproduction steps when possible. Do not open a public issue for unfixed vulnerabilities.

We aim to acknowledge reports within a few business days.

## Threat model (L0 library)

Spectra is an **in-process** observability library. It does **not** implement session authentication, Gauge permissions, or HTTP ACL. Hosts (Orbital / Lepton / Gauge) own query authorization.

| Trust assumption | Library behavior |
|------------------|------------------|
| Emit callers are in-process | Dynamic metric/table names are charset-validated; invalid names are dropped |
| Query DTOs may be attacker-controlled once hosts expose them | Identifiers validated; event `limit`/`offset` clamped; SQL literals escaped / NUL rejected |
| Connection URLs may contain secrets | Remote error mapping redacts URL userinfo |
| Operators control env | `SPECTRA_GATE=0` requires `SPECTRA_GATE_FORCE_OFF=1` to disable the emit gate |

Hosts should:

- Enforce `spectra.query.{table}` (or equivalent) before calling query APIs.
- Call [`mask_field_value`](spectra-core) before rendering fields classified as PII.
- Prefer Neutrino (or equivalent) for ClickHouse credentials rather than logging raw URLs.
- Keep ingest / readiness HTTP endpoints off the public internet (host/runtime concern).

Discoverable API docs: `cargo doc -p uf-spectra --open` → **Features** (validation, paging clamps, gate, classification).

## Supply-chain checks

Maintainers run `cargo deny check` (see [`deny.toml`](deny.toml)) in CI. Optional local guidance:

```bash
cargo install cargo-deny --locked
cargo deny check
# optional complementary scan
cargo audit
```

Infra bootstrap scripts verify SHA-256 digests for pinned TensorBase / rustup downloads; see [`infra/aws/checksums/SHA256SUMS`](infra/aws/checksums/SHA256SUMS).
