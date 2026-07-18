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
/// [`Internal`](Self::Internal) with their original message.
///
/// # Examples
///
/// ```
/// use spectra_core::Error;
///
/// let error = Error::Internal("metrics backend is required".into());
/// assert_eq!(error.to_string(), "metrics backend is required");
///
/// match error {
///     Error::Internal(message) => assert!(message.contains("backend")),
///     Error::Io(_) | Error::Json(_) => unreachable!(),
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
    /// Catch-all internal error with a message.
    #[error("{0}")]
    Internal(String),
}
