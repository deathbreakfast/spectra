//! Remote engine transport security policy for ClickHouse-protocol connect paths.

use spectra_core::{Error, Result};

/// Environment opt-in for plaintext remote endpoints (development/CI only).
pub const ALLOW_INSECURE_REMOTE_ENV: &str = "SPECTRA_ALLOW_INSECURE_REMOTE";

/// How Spectra may connect to a remote ClickHouse-compatible engine.
///
/// Production hosts should use [`Self::RequireTls`]. Plaintext endpoints
/// (`http://`, `tcp://`) require an explicit [`Self::AllowInsecurePlaintext`] opt-in via
/// [`ALLOW_INSECURE_REMOTE_ENV`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RemoteTransportSecurity {
    /// Reject plaintext remote URLs; prefer `https://` or `tcp+tls://`.
    #[default]
    RequireTls,
    /// Development/CI only: allow `http://` and `tcp://` endpoints.
    AllowInsecurePlaintext,
}

impl RemoteTransportSecurity {
    /// Load from [`ALLOW_INSECURE_REMOTE_ENV`] (`1`/`true`/`yes` → insecure; otherwise require TLS).
    #[must_use]
    pub fn from_env() -> Self {
        match std::env::var(ALLOW_INSECURE_REMOTE_ENV).as_deref() {
            Ok("1" | "true" | "TRUE" | "yes" | "YES") => Self::AllowInsecurePlaintext,
            _ => Self::RequireTls,
        }
    }

    /// Returns true when plaintext remote endpoints are permitted.
    #[must_use]
    pub const fn allows_plaintext(self) -> bool {
        matches!(self, Self::AllowInsecurePlaintext)
    }

    /// Fail closed when `url` looks like plaintext and insecure opt-in is absent.
    ///
    /// TLS-oriented schemes: `https://`, `tcp+tls://`, `tcps://`. Plaintext
    /// (`http://`, `tcp://`, unknown) require [`Self::AllowInsecurePlaintext`].
    ///
    /// When plaintext is allowed and the URL is non-TLS, emits an explicit `tracing` warning.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] when plaintext would be used without opt-in.
    pub fn check_url(self, url: &str) -> Result<()> {
        if url_looks_tls(url) {
            return Ok(());
        }
        if self.allows_plaintext() {
            tracing::warn!(
                SPECTRA_ALLOW_INSECURE_REMOTE = "1",
                url_scheme = url_scheme_label(url),
                "plaintext remote Spectra URL allowed via SPECTRA_ALLOW_INSECURE_REMOTE \
                 (development/CI only; prefer https:// or tcp+tls:// in production)"
            );
            return Ok(());
        }
        Err(Error::config(format!(
            "plaintext remote URL rejected under RemoteTransportSecurity::RequireTls \
             (url looks non-TLS). Use https:// or tcp+tls://, or set {ALLOW_INSECURE_REMOTE_ENV}=1 \
             for development/CI only"
        )))
    }
}

fn url_looks_tls(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("tcp+tls://") || lower.starts_with("tcps://")
}

fn url_scheme_label(url: &str) -> &str {
    let trimmed = url.trim();
    match trimmed.find("://") {
        Some(idx) => &trimmed[..idx],
        None => "unknown",
    }
}

/// Classify a remote URL for client construction after security checks pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemoteUrlKind {
    /// HTTP(S) ClickHouse protocol (`http://` / `https://`).
    Http,
    /// Native TCP (`tcp://` plaintext or `tcp+tls://` / `tcps://` with TLS).
    Native { secure: bool },
}

impl RemoteUrlKind {
    /// Parse the scheme for a supported remote URL.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] when the scheme is not a supported Spectra remote form.
    pub(crate) fn parse(url: &str) -> Result<(Self, &str)> {
        let trimmed = url.trim();
        let lower = trimmed.to_ascii_lowercase();
        if let Some(addr) = strip_scheme_ci(trimmed, &lower, "tcp+tls://") {
            return Ok((Self::Native { secure: true }, addr));
        }
        if let Some(addr) = strip_scheme_ci(trimmed, &lower, "tcps://") {
            return Ok((Self::Native { secure: true }, addr));
        }
        if let Some(addr) = strip_scheme_ci(trimmed, &lower, "tcp://") {
            return Ok((Self::Native { secure: false }, addr));
        }
        if lower.starts_with("https://") || lower.starts_with("http://") {
            return Ok((Self::Http, trimmed));
        }
        Err(Error::config(format!(
            "unsupported remote URL scheme (expected http(s):// or tcp:// / tcp+tls://): \
             {}",
            url_scheme_label(trimmed)
        )))
    }
}

fn strip_scheme_ci<'a>(original: &'a str, lower: &str, scheme: &str) -> Option<&'a str> {
    if lower.starts_with(scheme) {
        Some(&original[scheme.len()..])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_tls_rejects_http_plaintext() {
        let err = RemoteTransportSecurity::RequireTls
            .check_url("http://127.0.0.1:8123")
            .expect_err("plaintext");
        assert!(err.to_string().contains("plaintext"));
        assert!(err.to_string().contains(ALLOW_INSECURE_REMOTE_ENV));
    }

    #[test]
    fn require_tls_rejects_tcp_plaintext() {
        let err = RemoteTransportSecurity::RequireTls
            .check_url("tcp://127.0.0.1:9528")
            .expect_err("plaintext");
        assert!(err.to_string().contains("plaintext"));
    }

    #[test]
    fn require_tls_accepts_https() {
        RemoteTransportSecurity::RequireTls
            .check_url("https://clickhouse.example:8443")
            .expect("https ok");
    }

    #[test]
    fn require_tls_accepts_tcp_tls() {
        RemoteTransportSecurity::RequireTls
            .check_url("tcp+tls://127.0.0.1:9440")
            .expect("tcp+tls ok");
        RemoteTransportSecurity::RequireTls
            .check_url("tcps://127.0.0.1:9440")
            .expect("tcps ok");
    }

    #[test]
    fn insecure_allows_http_plaintext() {
        RemoteTransportSecurity::AllowInsecurePlaintext
            .check_url("http://127.0.0.1:8123")
            .expect("insecure ok");
    }

    #[test]
    fn insecure_allows_tcp_plaintext() {
        RemoteTransportSecurity::AllowInsecurePlaintext
            .check_url("tcp://127.0.0.1:9528")
            .expect("insecure ok");
    }

    #[test]
    fn parse_native_secure_and_http() {
        assert_eq!(
            RemoteUrlKind::parse("tcp+tls://db:9440").unwrap(),
            (RemoteUrlKind::Native { secure: true }, "db:9440")
        );
        assert_eq!(
            RemoteUrlKind::parse("TCP://db:9528").unwrap(),
            (RemoteUrlKind::Native { secure: false }, "db:9528")
        );
        let (kind, url) = RemoteUrlKind::parse("https://db:8443/").unwrap();
        assert_eq!(kind, RemoteUrlKind::Http);
        assert_eq!(url, "https://db:8443/");
    }

    #[test]
    fn parse_rejects_unknown_scheme() {
        let err = RemoteUrlKind::parse("ftp://db").expect_err("unknown");
        assert!(err.to_string().contains("unsupported"));
    }
}
