use std::error::Error as StdError;

use thiserror::Error;

/// Result type returned by Spectra operations.
///
/// # Examples
///
/// ```
/// use spectra_core::Result;
///
/// fn validate_name(name: &str) -> Result<()> {
///     if name.is_empty() {
///         return Err(spectra_core::Error::Internal("metric name is empty".into()));
///     }
///     Ok(())
/// }
///
/// assert!(validate_name("cache_hits").is_ok());
/// ```
pub type Result<T> = std::result::Result<T, Error>;

/// Errors returned by Spectra storage, routing, serialization, and I/O paths.
///
/// Backend-specific failures that do not map to I/O or JSON are reported as
/// [`Storage`](Self::Storage) (with an optional [`Error::source`] chain) or
/// [`Config`](Self::Config) for builder/wiring mistakes. [`Internal`](Self::Internal)
/// is reserved for invariant violations and parse bugs.
///
/// # Examples
///
/// ```
/// use spectra_core::Error;
///
/// let error = Error::config("metrics backend is required");
/// assert_eq!(error.to_string(), "config error: metrics backend is required");
///
/// match error {
///     Error::Config(message) => assert!(message.contains("backend")),
///     Error::Io(_) | Error::Json(_) | Error::Storage { .. } | Error::Internal(_) => {
///         unreachable!()
///     }
/// }
/// ```
#[derive(Debug, Error)]
pub enum Error {
    /// Underlying filesystem or stream I/O failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// JSON serialization or deserialization failure.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    /// Storage backend failure (SQLite, remote engine, etc.).
    #[error("storage error: {message}")]
    Storage {
        /// Human-readable summary (stable for logs and hosts).
        message: String,
        /// Optional underlying backend error for `Error::source` chains.
        #[source]
        source: Option<Box<dyn StdError + Send + Sync>>,
    },
    /// Builder, wiring, or configuration mistake.
    #[error("config error: {0}")]
    Config(String),
    /// Catch-all for invariant violations and unexpected parse bugs.
    #[error("{0}")]
    Internal(String),
}

impl Error {
    /// Storage failure without an underlying source.
    pub fn storage(message: impl Into<String>) -> Self {
        Self::Storage {
            message: message.into(),
            source: None,
        }
    }

    /// Storage failure wrapping an underlying error.
    pub fn storage_source(
        message: impl Into<String>,
        source: impl StdError + Send + Sync + 'static,
    ) -> Self {
        Self::Storage {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Configuration / builder failure.
    pub fn config(message: impl Into<String>) -> Self {
        Self::Config(message.into())
    }

    /// Invariant or unexpected internal failure.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as StdError;

    use super::Error;

    #[test]
    fn storage_source_preserves_chain() {
        let err = Error::storage_source("sqlite open failed", std::io::Error::other("disk full"));
        match &err {
            Error::Storage {
                message,
                source: Some(_),
            } => assert!(message.contains("sqlite")),
            _ => panic!("expected Storage with source"),
        }
        assert!(err.source().is_some());
    }

    #[test]
    fn config_display() {
        let err = Error::config("metrics_backend is required");
        assert_eq!(err.to_string(), "config error: metrics_backend is required");
    }
}
