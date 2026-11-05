use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::routing::{get, post};
use common_axum::{ApiResult, Json, Query, REQUEST_ID_HEADER, ServerConfig, ValidatedJson, app};
use common_error::{Error, Result};
use common_lifecycle::HealthRegistry;
use common_types::{PageRequest, Validate};
use http_body_util::BodyExt;
use serde::Deserialize;
use serde_json::{Value, json};
use tower::ServiceExt;

#[derive(Deserialize)]
struct CreateUser {
    name: String,
}

impl Validate for CreateUser {
    fn validate(&self) -> Result<()> {
        if self.name.len() < 3 {
            return Err(Error::invalid_input("name must have at least 3 characters"));
        }
        Ok(())
    }
}

async fn get_user() -> ApiResult<Json<Value>> {
    Err(Error::not_found("user 42")
        .with_code("USER_NOT_FOUND")
        .into())
}

async fn broken() -> ApiResult<()> {
    Err(Error::internal("db password is hunter2").into())
}

async fn create(ValidatedJson(body): ValidatedJson<CreateUser>) -> Json<Value> {
    Json(json!({ "name": body.name }))
}

async fn echo(Json(body): Json<Value>) -> Json<Value> {
    Json(body)
}

async fn list(Query(page): Query<PageRequest>) -> Json<Value> {
    Json(json!({ "limit": page.limit(), "offset": page.offset() }))
}

async fn slow() -> &'static str {
    tokio::time::sleep(Duration::from_secs(60)).await;
    "done"
}

async fn panics() -> &'static str {
    panic!("kaboom")
}

fn routes() -> Router {
    Router::new()
        .route("/users/{id}", get(get_user))
        .route("/users", post(create).get(list))
        .route("/echo", post(echo))
        .route("/broken", get(broken))
        .route("/slow", get(slow))
        .route("/panic", get(panics))
}

fn test_app(health: HealthRegistry) -> Router {
    let cfg = ServerConfig {
        request_timeout_secs: 1,
        ..ServerConfig::default()
    };
    app(routes(), &cfg, health).unwrap()
}

async fn send(app: Router, req: Request<Body>) -> (StatusCode, axum::http::HeaderMap, Value) {
    let res = app.oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, headers, body)
}

fn get_req(uri: &str) -> Request<Body> {
    Request::get(uri).body(Body::empty()).unwrap()
}

fn json_req(uri: &str, body: &str) -> Request<Body> {
    Request::post(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

#[tokio::test]
async fn domain_error_becomes_problem_json() {
    let (status, headers, body) = send(test_app(HealthRegistry::new()), get_req("/users/42")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(headers[header::CONTENT_TYPE], "application/problem+json");
    assert_eq!(body["code"], "USER_NOT_FOUND");
    assert_eq!(body["detail"], "user 42");
    assert_eq!(body["title"], "Not Found");
    assert!(headers.contains_key(REQUEST_ID_HEADER));
}

#[tokio::test]
async fn internal_error_detail_is_hidden() {
    let (status, _, body) = send(test_app(HealthRegistry::new()), get_req("/broken")).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["detail"], "internal server error");
    assert_eq!(body["code"], "INTERNAL");
}

#[tokio::test]
async fn incoming_request_id_is_propagated() {
    let req = Request::get("/users/1")
        .header(REQUEST_ID_HEADER, "abc-123")
        .body(Body::empty())
        .unwrap();
    let (_, headers, _) = send(test_app(HealthRegistry::new()), req).await;
    assert_eq!(headers[REQUEST_ID_HEADER], "abc-123");
}

#[tokio::test]
async fn unknown_route_and_method_are_problem_json() {
    let (status, _, body) = send(test_app(HealthRegistry::new()), get_req("/nope")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "NOT_FOUND");

    let req = Request::delete("/users").body(Body::empty()).unwrap();
    let (status, _, body) = send(test_app(HealthRegistry::new()), req).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(body["status"], 405);
}

#[tokio::test]
async fn json_rejections_keep_their_status() {
    let (status, _, body) = send(test_app(HealthRegistry::new()), json_req("/echo", "{")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_INPUT");

    let req = Request::post("/echo").body(Body::from("{}")).unwrap();
    let (status, _, _) = send(test_app(HealthRegistry::new()), req).await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);

    let (status, _, _) = send(
        test_app(HealthRegistry::new()),
        json_req("/users", r#"{"x":1}"#),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn validated_json_runs_validation() {
    let (status, _, body) = send(
        test_app(HealthRegistry::new()),
        json_req("/users", r#"{"name":"al"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["detail"], "name must have at least 3 characters");

    let (status, _, body) = send(
        test_app(HealthRegistry::new()),
        json_req("/users", r#"{"name":"alice"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "alice");
}

#[tokio::test]
async fn query_pagination_is_normalized() {
    let (status, _, body) = send(
        test_app(HealthRegistry::new()),
        get_req("/users?page=3&per_page=1000"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "limit": 100, "offset": 200 }));
}

#[tokio::test(start_paused = true)]
async fn slow_handler_times_out() {
    let (status, _, body) = send(test_app(HealthRegistry::new()), get_req("/slow")).await;
    assert_eq!(status, StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(body["code"], "TIMEOUT");
}

#[tokio::test]
async fn panic_becomes_500() {
    let (status, _, body) = send(test_app(HealthRegistry::new()), get_req("/panic")).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["detail"], "internal server error");
}

#[tokio::test]
async fn health_endpoints_reflect_checks() {
    let health = HealthRegistry::new();
    let (status, _, body) = send(test_app(health.clone()), get_req("/health/live")).await;
    assert_eq!((status, &body["status"]), (StatusCode::OK, &json!("up")));

    let (status, _, _) = send(test_app(health.clone()), get_req("/health/ready")).await;
    assert_eq!(status, StatusCode::OK);

    health.register("db", || async {
        Err(Error::internal("connection refused"))
    });
    let (status, _, body) = send(test_app(health), get_req("/health/ready")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["checks"][0]["name"], "db");
}
