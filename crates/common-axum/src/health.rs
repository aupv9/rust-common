use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use common_lifecycle::HealthRegistry;

pub fn health_routes(registry: HealthRegistry) -> Router {
    Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .with_state(registry)
}

async fn live() -> Response {
    axum::Json(serde_json::json!({ "status": "up" })).into_response()
}

async fn ready(State(registry): State<HealthRegistry>) -> Response {
    let report = registry.check().await;
    let status = if report.is_ready() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, axum::Json(report)).into_response()
}
