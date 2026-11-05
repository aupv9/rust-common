use crate::{Error, ErrorKind};

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        let (kind, message) = if err.is_timeout() {
            (ErrorKind::Timeout, "upstream request timed out")
        } else if err.is_connect() {
            (ErrorKind::Unavailable, "upstream unreachable")
        } else if let Some(status) = err.status() {
            (
                ErrorKind::from_http_status(status.as_u16()),
                "upstream returned an error status",
            )
        } else if err.is_decode() || err.is_body() {
            (ErrorKind::Internal, "invalid upstream response")
        } else if err.is_builder() {
            (ErrorKind::Internal, "invalid upstream request")
        } else {
            (ErrorKind::Unavailable, "upstream request failed")
        };
        Error::new(kind, message).with_source(err)
    }
}
