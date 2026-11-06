use std::any::Any;
use std::time::Duration;

use axum::body::Body;
use axum::error_handling::HandleErrorLayer;
use axum::extract::{DefaultBodyLimit, MatchedPath};
use axum::http::{HeaderName, HeaderValue, Request, Response, StatusCode};
use axum::response::IntoResponse;
use axum::{BoxError, Router};
use common_error::{Error, ErrorKind, Result};
use common_types::Uuid;
use tower::ServiceBuilder;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any as AnyOrigin, CorsLayer};
use tower_http::request_id::{
    MakeRequestId, PropagateRequestIdLayer, RequestId, SetRequestIdLayer,
};
use tower_http::trace::{OnResponse, TraceLayer};
use tracing::Span;

use crate::config::{CorsConfig, ServerConfig};
use crate::error::ApiError;

pub const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

pub fn with_defaults(router: Router, cfg: &ServerConfig) -> Result<Router> {
    let cors = cors_layer(&cfg.cors)?;
    let stack = ServiceBuilder::new()
        .layer(SetRequestIdLayer::new(REQUEST_ID_HEADER, MakeRequestIdV7))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(make_span as fn(&Request<Body>) -> Span)
                .on_request(())
                .on_response(LogResponse)
                .on_failure(()),
        )
        .layer(PropagateRequestIdLayer::new(REQUEST_ID_HEADER))
        .layer(CatchPanicLayer::custom(
            panic_response as fn(Box<dyn Any + Send>) -> _,
        ))
        .layer(HandleErrorLayer::new(handle_middleware_error))
        .timeout(cfg.request_timeout())
        .layer(CompressionLayer::new())
        .option_layer(cors);

    Ok(router
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
        .layer(DefaultBodyLimit::max(cfg.body_limit_bytes))
        .layer(stack))
}

#[derive(Clone, Copy)]
struct MakeRequestIdV7;

impl MakeRequestId for MakeRequestIdV7 {
    fn make_request_id<B>(&mut self, _: &Request<B>) -> Option<RequestId> {
        HeaderValue::from_str(&Uuid::now_v7().to_string())
            .ok()
            .map(RequestId::new)
    }
}

fn make_span(req: &Request<Body>) -> Span {
    let request_id = req
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map_or("unmatched", MatchedPath::as_str);

    let span = tracing::info_span!(
        "http.request",
        otel.name = format!("{} {route}", req.method()),
        otel.kind = "server",
        http.request.method = %req.method(),
        http.route = route,
        url.path = req.uri().path(),
        http.response.status_code = tracing::field::Empty,
        enduser.id = tracing::field::Empty,
        request_id,
    );

    #[cfg(feature = "otel")]
    common_telemetry::propagation::set_parent_from_headers(&span, req.headers());

    span
}

#[derive(Clone, Copy)]
struct LogResponse;

impl<B> OnResponse<B> for LogResponse {
    fn on_response(self, res: &Response<B>, latency: Duration, span: &Span) {
        let status = res.status().as_u16();
        span.record("http.response.status_code", status);
        let latency_ms = u64::try_from(latency.as_millis()).unwrap_or(u64::MAX);
        tracing::info!(status, latency_ms, "request completed");
    }
}

async fn handle_middleware_error(err: BoxError) -> ApiError {
    if err.is::<tower::timeout::error::Elapsed>() {
        ApiError::new(Error::new(ErrorKind::Timeout, "request timed out"))
    } else {
        ApiError::new(Error::internal("unhandled middleware error").with_source(err))
    }
}

fn panic_response(panic: Box<dyn Any + Send>) -> axum::response::Response {
    let message = match panic.downcast::<String>() {
        Ok(message) => *message,
        Err(panic) => panic
            .downcast::<&str>()
            .map_or_else(|_| "unknown panic".to_owned(), |m| (*m).to_owned()),
    };
    ApiError::new(Error::internal("handler panicked").with_source(Error::internal(message)))
        .into_response()
}

async fn not_found() -> ApiError {
    ApiError::new(Error::not_found("route not found"))
}

async fn method_not_allowed() -> ApiError {
    ApiError::new(Error::invalid_input("method not allowed"))
        .with_status(StatusCode::METHOD_NOT_ALLOWED)
}

fn cors_layer(cfg: &CorsConfig) -> Result<Option<CorsLayer>> {
    if cfg.allowed_origins.is_empty() {
        return Ok(None);
    }
    if cfg.allowed_origins.iter().any(|o| o == "*") {
        return Ok(Some(CorsLayer::permissive()));
    }
    let origins = cfg
        .allowed_origins
        .iter()
        .map(|o| {
            HeaderValue::from_str(o).map_err(|e| {
                Error::new(ErrorKind::Config, format!("invalid CORS origin `{o}`")).with_source(e)
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Some(
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods(AnyOrigin)
            .allow_headers(AnyOrigin),
    ))
}
