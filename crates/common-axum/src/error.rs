use std::error::Error as StdError;

use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use common_error::{Error, ErrorKind};
use serde::Serialize;

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug)]
pub struct ApiError {
    error: Error,
    status: StatusCode,
}

impl ApiError {
    #[must_use]
    pub fn new(error: Error) -> Self {
        let status = status_for(error.kind());
        Self { error, status }
    }

    #[must_use]
    pub fn with_status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }

    #[must_use]
    pub fn status(&self) -> StatusCode {
        self.status
    }

    #[must_use]
    pub fn error(&self) -> &Error {
        &self.error
    }

    #[must_use]
    pub fn into_inner(self) -> Error {
        self.error
    }

    pub(crate) fn rejection(status: StatusCode, message: String) -> Self {
        let kind = match status {
            StatusCode::UNAUTHORIZED => ErrorKind::Unauthorized,
            StatusCode::FORBIDDEN => ErrorKind::Forbidden,
            StatusCode::NOT_FOUND => ErrorKind::NotFound,
            s if s.is_server_error() => ErrorKind::Internal,
            _ => ErrorKind::InvalidInput,
        };
        Self::new(Error::new(kind, message)).with_status(status)
    }
}

impl<E: Into<Error>> From<E> for ApiError {
    fn from(err: E) -> Self {
        Self::new(err.into())
    }
}

#[must_use]
pub fn status_for(kind: ErrorKind) -> StatusCode {
    match kind {
        ErrorKind::NotFound => StatusCode::NOT_FOUND,
        ErrorKind::InvalidInput => StatusCode::BAD_REQUEST,
        ErrorKind::Unauthorized => StatusCode::UNAUTHORIZED,
        ErrorKind::Forbidden => StatusCode::FORBIDDEN,
        ErrorKind::Conflict => StatusCode::CONFLICT,
        ErrorKind::RateLimited => StatusCode::TOO_MANY_REQUESTS,
        ErrorKind::Timeout => StatusCode::GATEWAY_TIMEOUT,
        ErrorKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[derive(Serialize)]
struct Problem<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    title: &'a str,
    status: u16,
    detail: &'a str,
    code: &'a str,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status;
        let error = &self.error;

        if status.is_server_error() {
            tracing::error!(status = status.as_u16(), code = error.code(), error = %error, cause = %source_chain(error), "request failed");
        } else {
            tracing::debug!(status = status.as_u16(), code = error.code(), error = %error, "request rejected");
        }

        let detail = if status == StatusCode::INTERNAL_SERVER_ERROR {
            "internal server error"
        } else {
            error.message()
        };
        let problem = Problem {
            kind: "about:blank",
            title: status.canonical_reason().unwrap_or("Error"),
            status: status.as_u16(),
            detail,
            code: error.code(),
        };

        match serde_json::to_vec(&problem) {
            Ok(body) => (
                status,
                [(header::CONTENT_TYPE, "application/problem+json")],
                body,
            )
                .into_response(),
            Err(_) => status.into_response(),
        }
    }
}

fn source_chain(err: &Error) -> String {
    let mut parts = Vec::new();
    let mut current = err.source();
    while let Some(source) = current {
        parts.push(source.to_string());
        current = source.source();
    }
    parts.join(": ")
}
