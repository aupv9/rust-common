use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    NotFound,
    InvalidInput,
    Unauthorized,
    Forbidden,
    Conflict,
    RateLimited,
    Timeout,
    Unavailable,
    Config,
    Internal,
}

impl ErrorKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "NOT_FOUND",
            Self::InvalidInput => "INVALID_INPUT",
            Self::Unauthorized => "UNAUTHORIZED",
            Self::Forbidden => "FORBIDDEN",
            Self::Conflict => "CONFLICT",
            Self::RateLimited => "RATE_LIMITED",
            Self::Timeout => "TIMEOUT",
            Self::Unavailable => "UNAVAILABLE",
            Self::Config => "CONFIG",
            Self::Internal => "INTERNAL",
        }
    }

    #[must_use]
    pub const fn is_retryable(self) -> bool {
        matches!(self, Self::RateLimited | Self::Timeout | Self::Unavailable)
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
