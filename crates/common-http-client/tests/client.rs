use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;
use common_error::ErrorKind;
use common_http_client::{HttpClient, HttpClientConfig};
use common_resilience::{CircuitBreakerConfig, CircuitState, RetryPolicy};
use serde_json::{Value, json};

#[derive(Clone, Default)]
struct Counters {
    flaky: Arc<AtomicU32>,
    always_down: Arc<AtomicU32>,
    post: Arc<AtomicU32>,
}

async fn flaky(State(c): State<Counters>) -> impl IntoResponse {
    if c.flaky.fetch_add(1, Ordering::SeqCst) < 2 {
        StatusCode::SERVICE_UNAVAILABLE.into_response()
    } else {
        axum::Json(json!({ "ok": true })).into_response()
    }
}

async fn always_down(State(c): State<Counters>) -> StatusCode {
    c.always_down.fetch_add(1, Ordering::SeqCst);
    StatusCode::SERVICE_UNAVAILABLE
}

async fn post_down(State(c): State<Counters>) -> StatusCode {
    c.post.fetch_add(1, Ordering::SeqCst);
    StatusCode::SERVICE_UNAVAILABLE
}

async fn problem() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        [(header::CONTENT_TYPE, "application/problem+json")],
        r#"{"type":"about:blank","title":"Not Found","status":404,"detail":"user 42","code":"USER_NOT_FOUND"}"#,
    )
}

async fn slow() -> &'static str {
    tokio::time::sleep(Duration::from_secs(5)).await;
    "late"
}

async fn spawn_server() -> (String, Counters) {
    let counters = Counters::default();
    let app = Router::new()
        .route("/flaky", get(flaky))
        .route("/down", get(always_down).post(post_down))
        .route("/problem", get(problem))
        .route("/slow", get(slow))
        .with_state(counters.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}"), counters)
}

fn config() -> HttpClientConfig {
    HttpClientConfig {
        retry: RetryPolicy {
            max_attempts: 3,
            initial_delay_ms: 1,
            jitter: false,
            ..RetryPolicy::default()
        },
        ..HttpClientConfig::default()
    }
}

#[tokio::test]
async fn retries_idempotent_requests_until_success() {
    let (base, counters) = spawn_server().await;
    let client = HttpClient::new("test", &config()).unwrap();

    let body: Value = client
        .send_json(client.get(format!("{base}/flaky")))
        .await
        .unwrap();
    assert_eq!(body, json!({ "ok": true }));
    assert_eq!(counters.flaky.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn non_idempotent_requests_are_not_retried() {
    let (base, counters) = spawn_server().await;
    let client = HttpClient::new("test", &config()).unwrap();

    let err = client
        .send(client.post(format!("{base}/down")).body("x"))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Unavailable);
    assert_eq!(counters.post.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn problem_json_code_and_detail_are_propagated() {
    let (base, _) = spawn_server().await;
    let client = HttpClient::new("test", &config()).unwrap();

    let err = client
        .send(client.get(format!("{base}/problem")))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
    assert_eq!(err.code(), "USER_NOT_FOUND");
    assert_eq!(err.message(), "user 42");
}

#[tokio::test]
async fn timeout_maps_to_timeout_kind() {
    let (base, _) = spawn_server().await;
    let cfg = HttpClientConfig {
        timeout_secs: 1,
        retry: RetryPolicy::none(),
        ..config()
    };
    let client = HttpClient::new("test", &cfg).unwrap();

    let err = client
        .send(client.get(format!("{base}/slow")))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Timeout);
}

#[tokio::test]
async fn connection_refused_is_unavailable() {
    let client = HttpClient::new(
        "test",
        &HttpClientConfig {
            retry: RetryPolicy::none(),
            ..config()
        },
    )
    .unwrap();
    let err = client
        .send(client.get("http://127.0.0.1:1/"))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Unavailable);
}

#[tokio::test]
async fn circuit_breaker_opens_and_short_circuits() {
    let (base, counters) = spawn_server().await;
    let cfg = HttpClientConfig {
        retry: RetryPolicy::none(),
        circuit_breaker: Some(CircuitBreakerConfig {
            failure_threshold: 2,
            open_secs: 60,
        }),
        ..config()
    };
    let client = HttpClient::new("payments", &cfg).unwrap();

    for _ in 0..2 {
        client
            .send(client.get(format!("{base}/down")))
            .await
            .unwrap_err();
    }
    assert_eq!(
        client.circuit_breaker().unwrap().state(),
        CircuitState::Open
    );

    let err = client
        .send(client.get(format!("{base}/down")))
        .await
        .unwrap_err();
    assert_eq!(err.message(), "circuit `payments` is open");
    assert_eq!(counters.always_down.load(Ordering::SeqCst), 2);
}
