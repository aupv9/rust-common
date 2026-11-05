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
    pub const fn from_http_status(status: u16) -> Self {
        match status {
            400 | 422 => Self::InvalidInput,
            401 => Self::Unauthorized,
            403 => Self::Forbidden,
            404 | 410 => Self::NotFound,
            409 | 412 => Self::Conflict,
            429 => Self::RateLimited,
            408 | 504 => Self::Timeout,
            502 | 503 => Self::Unavailable,
            _ => Self::Internal,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_status_mapping() {
        assert_eq!(ErrorKind::from_http_status(404), ErrorKind::NotFound);
        assert_eq!(ErrorKind::from_http_status(503), ErrorKind::Unavailable);
        assert_eq!(ErrorKind::from_http_status(504), ErrorKind::Timeout);
        assert_eq!(ErrorKind::from_http_status(500), ErrorKind::Internal);
        assert!(ErrorKind::from_http_status(429).is_retryable());
    }
}
