# Security policy

## Supported versions

Spectra is published as git tags (for example `v0.1.0`). Security fixes land on the default branch and are released in subsequent tags.

## Reporting a vulnerability

Please report security issues privately via GitHub Security Advisories for this repository:

https://github.com/unified-field-dev/spectra/security/advisories/new

Include a clear description, impact assessment, and reproduction steps when possible. Do not open a public issue for unfixed vulnerabilities.

We aim to acknowledge reports within a few business days.

## Supply-chain checks

Maintainers run `cargo deny check` (see [`deny.toml`](deny.toml)) in CI. Optional local guidance:

```bash
cargo install cargo-deny --locked
cargo deny check
# optional complementary scan
cargo audit
```

Infra bootstrap scripts verify SHA-256 digests for pinned TensorBase / rustup downloads; see [`infra/aws/checksums/SHA256SUMS`](infra/aws/checksums/SHA256SUMS).
