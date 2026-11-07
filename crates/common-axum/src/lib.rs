#[cfg(feature = "auth")]
mod auth;
mod config;
mod error;
mod extract;
mod health;
#[cfg(feature = "otel")]
mod metrics;
mod middleware;
mod server;

#[cfg(feature = "auth")]
pub use auth::{
    API_KEY_HEADER, Auth, AuthRejection, RequireLayer, RequireService, require, with_auth,
};
pub use axum;
pub use config::{CorsConfig, ServerConfig};
pub use error::{ApiError, ApiResult, status_for};
pub use extract::{Json, Path, Query, ValidatedJson};
pub use health::health_routes;
#[cfg(feature = "otel")]
pub use metrics::{HttpMetricsFuture, HttpMetricsLayer, HttpMetricsService};
pub use middleware::{REQUEST_ID_HEADER, with_defaults};
pub use server::{app, serve};
