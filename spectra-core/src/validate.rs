//! Input validation for Spectra names and query paging bounds.

use crate::error::{Error, Result};

/// Maximum rows returned by a single event query (inclusive).
pub const MAX_EVENT_QUERY_LIMIT: u32 = 1000;

/// Maximum event query offset (inclusive).
pub const MAX_EVENT_QUERY_OFFSET: u32 = 1_000_000;

/// Maximum length for metric, table, and JSON field identifier tokens.
pub const MAX_SPECTRA_IDENT_LEN: usize = 128;

/// Validate a metric name, event table name, or JSON field key used in SQL builders.
///
/// Accepted form: non-empty, at most [`MAX_SPECTRA_IDENT_LEN`] bytes, matching
/// `^[A-Za-z_][A-Za-z0-9_.]*$` (no whitespace, quotes, or control characters).
///
/// # Errors
///
/// Returns [`Error::Config`] when the token is empty, too long, or contains disallowed characters.
///
/// # Examples
///
/// ```
/// use spectra_core::validate_spectra_ident;
///
/// assert!(validate_spectra_ident("cache_hits").is_ok());
/// assert!(validate_spectra_ident("metrics.latency").is_ok());
/// assert!(validate_spectra_ident("msg; DROP").is_err());
/// assert!(validate_spectra_ident("").is_err());
/// ```
pub fn validate_spectra_ident(name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(Error::config(
            "invalid spectra identifier: empty (operation=validate_spectra_ident, reason=empty)",
        ));
    }
    if trimmed.len() > MAX_SPECTRA_IDENT_LEN {
        return Err(Error::config(
            "invalid spectra identifier: too long (operation=validate_spectra_ident, reason=too_long)",
        ));
    }
    if trimmed.contains('\0') {
        return Err(Error::config(
            "invalid spectra identifier: nul byte (operation=validate_spectra_ident, reason=nul)",
        ));
    }
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return Err(Error::config(
            "invalid spectra identifier: empty (operation=validate_spectra_ident, reason=empty)",
        ));
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(Error::config(
            "invalid spectra identifier: bad start (operation=validate_spectra_ident, reason=charset)",
        ));
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.') {
        return Err(Error::config(
            "invalid spectra identifier: bad charset (operation=validate_spectra_ident, reason=charset)",
        ));
    }
    if trimmed != name {
        // Reject leading/trailing whitespace rather than silently trimming into SQL.
        return Err(Error::config(
            "invalid spectra identifier: surrounding whitespace (operation=validate_spectra_ident, reason=whitespace)",
        ));
    }
    Ok(())
}

/// Whether [`validate_spectra_ident`] would succeed (no allocation / error construction).
#[must_use]
pub fn is_valid_spectra_ident(name: &str) -> bool {
    validate_spectra_ident(name).is_ok()
}

/// Clamp event-query `limit` / `offset` to documented maxima.
///
/// Missing `limit` defaults to [`MAX_EVENT_QUERY_LIMIT`]. Missing `offset` defaults to `0`.
///
/// # Examples
///
/// ```
/// use spectra_core::{clamp_event_paging, MAX_EVENT_QUERY_LIMIT};
///
/// assert_eq!(clamp_event_paging(Some(50), None), (50, 0));
/// assert_eq!(
///     clamp_event_paging(Some(u32::MAX), Some(u32::MAX)).0,
///     MAX_EVENT_QUERY_LIMIT
/// );
/// ```
#[must_use]
pub fn clamp_event_paging(limit: Option<u32>, offset: Option<u32>) -> (u32, u32) {
    let limit = limit
        .unwrap_or(MAX_EVENT_QUERY_LIMIT)
        .min(MAX_EVENT_QUERY_LIMIT);
    let offset = offset.unwrap_or(0).min(MAX_EVENT_QUERY_OFFSET);
    (limit, offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_common_schema_names() {
        assert!(validate_spectra_ident("cache_hits").is_ok());
        assert!(validate_spectra_ident("platform_smoke_event").is_ok());
        assert!(validate_spectra_ident("metrics.latency").is_ok());
        assert!(validate_spectra_ident("_private").is_ok());
    }

    #[test]
    fn rejects_injection_shaped_tokens() {
        assert!(validate_spectra_ident("").is_err());
        assert!(validate_spectra_ident("   ").is_err());
        assert!(validate_spectra_ident("msg; DROP").is_err());
        assert!(validate_spectra_ident("foo bar").is_err());
        assert!(validate_spectra_ident("a'b").is_err());
        assert!(validate_spectra_ident("x--").is_err());
        assert!(validate_spectra_ident("a\0b").is_err());
        assert!(validate_spectra_ident("  cache_hits  ").is_err());
        assert!(validate_spectra_ident(&"a".repeat(MAX_SPECTRA_IDENT_LEN + 1)).is_err());
    }

    #[test]
    fn clamp_honors_maxima() {
        assert_eq!(clamp_event_paging(None, None), (MAX_EVENT_QUERY_LIMIT, 0));
        assert_eq!(clamp_event_paging(Some(10), Some(5)), (10, 5));
        assert_eq!(
            clamp_event_paging(Some(u32::MAX), Some(u32::MAX)),
            (MAX_EVENT_QUERY_LIMIT, MAX_EVENT_QUERY_OFFSET)
        );
    }
}
