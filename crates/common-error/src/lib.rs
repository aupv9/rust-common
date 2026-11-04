//! Shared error type used across all `common-*` crates.

use std::borrow::Cow;
use std::fmt;

/// Boxed error usable as an error source.
pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Convenience result alias using [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// High-level category of an [`Error`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The requested resource does not exist.
    NotFound,
    /// The caller supplied invalid input.
    InvalidInput,
    /// The caller is not authenticated.
    Unauthorized,
    /// The caller is authenticated but not allowed.
    Forbidden,
    /// The operation conflicts with current state.
    Conflict,
    /// A dependency is temporarily unavailable.
    Unavailable,
    /// Configuration is missing or invalid.
    Config,
    /// Unexpected internal failure.
    Internal,
}

impl ErrorKind {
    /// Stable, machine-readable name for this kind.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::InvalidInput => "invalid_input",
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::Conflict => "conflict",
            Self::Unavailable => "unavailable",
            Self::Config => "config",
            Self::Internal => "internal",
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Common error carrying a kind, a message, and an optional source.
#[derive(Debug, thiserror::Error)]
#[error("{kind}: {message}")]
pub struct Error {
    kind: ErrorKind,
    message: Cow<'static, str>,
    #[source]
    source: Option<BoxError>,
}

impl Error {
    /// Creates a new error of the given kind.
    pub fn new(kind: ErrorKind, message: impl Into<Cow<'static, str>>) -> Self {
        Self {
            kind,
            message: message.into(),
            source: None,
        }
    }

    /// Attaches an underlying cause.
    #[must_use]
    pub fn with_source(mut self, source: impl Into<BoxError>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Returns the error kind.
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Returns the human-readable message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Shorthand for [`ErrorKind::NotFound`].
    pub fn not_found(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(ErrorKind::NotFound, message)
    }

    /// Shorthand for [`ErrorKind::InvalidInput`].
    pub fn invalid_input(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(ErrorKind::InvalidInput, message)
    }

    /// Shorthand for [`ErrorKind::Internal`].
    pub fn internal(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(ErrorKind::Internal, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    #[test]
    fn display_includes_kind_and_message() {
        let err = Error::not_found("user 42");
        assert_eq!(err.to_string(), "not_found: user 42");
        assert_eq!(err.kind(), ErrorKind::NotFound);
    }

    #[test]
    fn source_is_preserved() {
        let io = std::io::Error::other("disk");
        let err = Error::internal("write failed").with_source(io);
        assert_eq!(err.source().unwrap().to_string(), "disk");
    }
}
