use std::borrow::Cow;

use crate::ErrorKind;

pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
#[error("{}: {message}", self.code())]
pub struct Error {
    kind: ErrorKind,
    code: Option<Cow<'static, str>>,
    message: Cow<'static, str>,
    #[source]
    source: Option<BoxError>,
}

impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<Cow<'static, str>>) -> Self {
        Self {
            kind,
            code: None,
            message: message.into(),
            source: None,
        }
    }

    #[must_use]
    pub fn with_code(mut self, code: impl Into<Cow<'static, str>>) -> Self {
        self.code = Some(code.into());
        self
    }

    #[must_use]
    pub fn with_source(mut self, source: impl Into<BoxError>) -> Self {
        self.source = Some(source.into());
        self
    }

    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    #[must_use]
    pub fn code(&self) -> &str {
        self.code.as_deref().unwrap_or(self.kind.as_str())
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn not_found(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(ErrorKind::NotFound, message)
    }

    pub fn invalid_input(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(ErrorKind::InvalidInput, message)
    }

    pub fn unauthorized(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(ErrorKind::Unauthorized, message)
    }

    pub fn forbidden(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(ErrorKind::Forbidden, message)
    }

    pub fn conflict(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(ErrorKind::Conflict, message)
    }

    pub fn internal(message: impl Into<Cow<'static, str>>) -> Self {
        Self::new(ErrorKind::Internal, message)
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;

    #[test]
    fn default_code_comes_from_kind() {
        let err = Error::not_found("user 42");
        assert_eq!(err.code(), "NOT_FOUND");
        assert_eq!(err.to_string(), "NOT_FOUND: user 42");
    }

    #[test]
    fn custom_code_overrides_kind_code() {
        let err = Error::not_found("user 42").with_code("USER_NOT_FOUND");
        assert_eq!(err.kind(), ErrorKind::NotFound);
        assert_eq!(err.to_string(), "USER_NOT_FOUND: user 42");
    }

    #[test]
    fn source_is_preserved() {
        let err = Error::internal("write failed").with_source(std::io::Error::other("disk"));
        assert_eq!(err.source().unwrap().to_string(), "disk");
    }
}
