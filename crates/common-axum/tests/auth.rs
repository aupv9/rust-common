#![cfg(feature = "auth")]

use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use axum::routing::{get, post};
use common_auth::{
    Algorithm, ApiKeyConfig, ApiKeyEntry, ApiKeyVerifier, AuthMethod, Authenticator, ClaimMapping,
    IssuerConfig, JwtConfig, JwtIssuer, JwtVerifier, KeySource, Principal, SigningKey,
    generate_api_key, scope,
};
use common_axum::{
    API_KEY_HEADER, ApiResult, Auth, Json, Path, ServerConfig, app, require, with_auth,
};
use common_lifecycle::HealthRegistry;
use common_types::Secret;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

const SECRET: &str = "0123456789abcdef0123456789abcdef";

struct Order {
    tenant: String,
    owner: String,
}

async fn me(Auth(principal): Auth) -> Json<Value> {
    Json(json!({
        "subject": principal.subject(),
        "tenant": principal.tenant(),
        "method": principal.method(),
    }))
}

async fn greeting(auth: Option<Auth>) -> String {
    match auth {
        Some(user) => format!("hello {}", user.subject()),
        None => "hello stranger".to_owned(),
    }
}

async fn create_order() -> StatusCode {
    StatusCode::CREATED
}

async fn edit_order(auth: Auth, Path(owner): Path<String>) -> ApiResult<StatusCode> {
    let order = Order {
        tenant: "acme".into(),
        owner,
    };
    auth.authorize(
        &|p: &Principal, o: &Order| {
            p.ensure_tenant(&o.tenant)?;
            if p.has_role("admin") {
                Ok(())
            } else {
                p.ensure_subject(&o.owner)
            }
        },
        &order,
    )?;
    Ok(StatusCode::NO_CONTENT)
}

fn routes() -> Router {
    Router::new()
        .route("/me", get(me))
        .route("/greeting", get(greeting))
        .route(
            "/orders",
            post(create_order).route_layer(require(scope("orders:write"))),
        )
        .route("/orders/{owner}", post(edit_order))
}

fn issuer() -> JwtIssuer {
    JwtIssuer::new(&IssuerConfig {
        issuer: "https://auth.example.com".into(),
        audiences: vec!["orders-api".into()],
        algorithm: Algorithm::HS256,
        key: SigningKey::Secret {
            secret: Secret::new(SECRET.into()),
        },
        key_id: None,
        ttl_secs: 60,
    })
    .unwrap()
}

fn test_app(api_key_hash: String) -> Router {
    let jwt = JwtVerifier::new(&JwtConfig {
        issuers: vec!["https://auth.example.com".into()],
        audiences: vec!["orders-api".into()],
        algorithms: vec![Algorithm::HS256],
        leeway_secs: 0,
        claims: ClaimMapping::default(),
        keys: KeySource::Secret {
            secret: Secret::new(SECRET.into()),
        },
    })
    .unwrap();
    let api_keys = ApiKeyVerifier::new(&ApiKeyConfig {
        keys: vec![ApiKeyEntry {
            id: "worker".into(),
            hash: api_key_hash,
            subject: "svc:worker".into(),
            tenant: None,
            roles: vec![],
            scopes: vec!["orders:write".into()],
        }],
    })
    .unwrap();
    let authenticator = Authenticator::new().with_jwt(jwt).with_api_keys(api_keys);
    app(
        with_auth(routes(), authenticator),
        &ServerConfig::default(),
        HealthRegistry::new(),
    )
    .unwrap()
}

fn default_app() -> Router {
    test_app(common_auth::hash_api_key("unused"))
}

fn token(principal: &Principal) -> String {
    issuer().issue(principal).unwrap().access_token
}

fn alice() -> Principal {
    Principal::new("alice", AuthMethod::Jwt).with_tenant("acme")
}

async fn send(app: Router, req: Request<Body>) -> (StatusCode, HeaderMap, Value) {
    let res = app.oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
    (status, headers, body)
}

fn get_with(uri: &str, token: Option<&str>) -> Request<Body> {
    let mut req = Request::get(uri);
    if let Some(token) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    req.body(Body::empty()).unwrap()
}

fn post_with(uri: &str, token: &str) -> Request<Body> {
    Request::post(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn missing_credentials_are_401_with_challenge() {
    let (status, headers, body) = send(default_app(), get_with("/me", None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(headers[header::WWW_AUTHENTICATE], "Bearer");
    assert_eq!(headers[header::CONTENT_TYPE], "application/problem+json");
    assert_eq!(body["code"], "AUTHENTICATION_REQUIRED");
}

#[tokio::test]
async fn valid_bearer_token_reaches_handler() {
    let (status, _, body) = send(default_app(), get_with("/me", Some(&token(&alice())))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({"subject": "alice", "tenant": "acme", "method": "jwt"})
    );
}

#[tokio::test]
async fn bad_tokens_are_rejected_before_the_handler() {
    let expired = issuer()
        .issue_with_ttl(&alice(), Duration::from_secs(1))
        .unwrap()
        .access_token;
    tokio::time::sleep(Duration::from_millis(2100)).await;
    let (status, headers, body) = send(default_app(), get_with("/greeting", Some(&expired))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "TOKEN_EXPIRED");
    assert_eq!(
        headers[header::WWW_AUTHENTICATE],
        r#"Bearer error="invalid_token""#
    );

    let (status, _, body) = send(default_app(), get_with("/greeting", Some("garbage"))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "INVALID_TOKEN");
    assert_eq!(body["detail"], "invalid token");
}

#[tokio::test]
async fn optional_auth_allows_anonymous() {
    let (status, _, body) = send(default_app(), get_with("/greeting", None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "hello stranger");

    let (_, _, body) = send(default_app(), get_with("/greeting", Some(&token(&alice())))).await;
    assert_eq!(body, "hello alice");
}

#[tokio::test]
async fn require_layer_enforces_scope() {
    let (status, _, body) = send(default_app(), post_with("/orders", &token(&alice()))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "PERMISSION_DENIED");
    assert_eq!(body["detail"], "requires scope `orders:write`");

    let writer = alice().with_scopes(["orders:write"]);
    let (status, _, _) = send(default_app(), post_with("/orders", &token(&writer))).await;
    assert_eq!(status, StatusCode::CREATED);

    let req = Request::post("/orders").body(Body::empty()).unwrap();
    let (status, _, _) = send(default_app(), req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn api_key_authenticates_services() {
    let (key, hash) = generate_api_key("sk").unwrap();
    let req = Request::post("/orders")
        .header(API_KEY_HEADER, key.expose())
        .body(Body::empty())
        .unwrap();
    let (status, _, _) = send(test_app(hash.clone()), req).await;
    assert_eq!(status, StatusCode::CREATED);

    let req = Request::get("/me")
        .header(API_KEY_HEADER, "sk_wrong")
        .body(Body::empty())
        .unwrap();
    let (status, _, body) = send(test_app(hash), req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "INVALID_API_KEY");
}

#[tokio::test]
async fn resource_policy_in_handler() {
    let bob = token(&Principal::new("bob", AuthMethod::Jwt).with_tenant("acme"));
    let (status, _, _) = send(default_app(), post_with("/orders/bob", &bob)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _, body) = send(default_app(), post_with("/orders/alice", &bob)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "NOT_OWNER");

    let admin = token(&alice().with_roles(["admin"]));
    let (status, _, _) = send(default_app(), post_with("/orders/bob", &admin)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let outsider = token(&Principal::new("bob", AuthMethod::Jwt).with_tenant("globex"));
    let (_, _, body) = send(default_app(), post_with("/orders/bob", &outsider)).await;
    assert_eq!(body["code"], "TENANT_MISMATCH");
}

#[tokio::test]
async fn auth_extractor_without_layer_is_a_server_error() {
    let app = app(routes(), &ServerConfig::default(), HealthRegistry::new()).unwrap();
    let (status, _, _) = send(app, get_with("/me", None)).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}
