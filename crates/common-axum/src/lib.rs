mod config;
mod error;
mod extract;
mod health;
mod middleware;
mod server;

pub use axum;
pub use config::{CorsConfig, ServerConfig};
pub use error::{ApiError, ApiResult, status_for};
pub use extract::{Json, Path, Query, ValidatedJson};
pub use health::health_routes;
pub use middleware::{REQUEST_ID_HEADER, with_defaults};
pub use server::{app, serve};
