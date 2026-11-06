#![cfg(all(feature = "jwks", feature = "issuer"))]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::signature::Ed25519KeyPair;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use common_auth::{
    Algorithm, AuthMethod, ClaimMapping, IssuerConfig, JwkSet, JwtConfig, JwtIssuer, JwtVerifier,
    KeySource, Principal, SigningKey,
};
use common_error::ErrorKind;
use common_http_client::{HttpClient, HttpClientConfig};
use common_resilience::RetryPolicy;
use common_types::Secret;

#[derive(Clone, Default)]
struct Idp {
    keys: Arc<Mutex<Option<JwkSet>>>,
    hits: Arc<AtomicUsize>,
}

impl Idp {
    fn publish(&self, keys: Option<JwkSet>) {
        *self.keys.lock().unwrap() = keys;
    }

    fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

async fn serve_jwks(State(idp): State<Idp>) -> Response {
    idp.hits.fetch_add(1, Ordering::SeqCst);
    match idp.keys.lock().unwrap().clone() {
        Some(set) => axum::Json(set).into_response(),
        None => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

async fn start(idp: Idp) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/jwks.json", get(serve_jwks))
        .with_state(idp);
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}/jwks.json")
}

fn issuer(kid: &str) -> JwtIssuer {
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    let pem = format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
        STANDARD.encode(pkcs8.as_ref())
    );
    JwtIssuer::new(&IssuerConfig {
        issuer: "https://idp.example.com".into(),
        audiences: vec!["orders-api".into()],
        algorithm: Algorithm::EdDSA,
        key: SigningKey::PrivateKeyPem {
            pem: Secret::new(pem),
        },
        key_id: Some(kid.into()),
        ttl_secs: 60,
    })
    .unwrap()
}

fn verifier(url: String, cache_secs: u64, min_refresh_secs: u64) -> JwtVerifier {
    let config = JwtConfig {
        issuers: vec!["https://idp.example.com".into()],
        audiences: vec!["orders-api".into()],
        algorithms: vec![Algorithm::EdDSA],
        leeway_secs: 0,
        claims: ClaimMapping::default(),
        keys: KeySource::Jwks {
            url,
            cache_secs,
            min_refresh_secs,
        },
    };
    let client = HttpClient::new(
        "jwks",
        &HttpClientConfig {
            retry: RetryPolicy::none(),
            ..HttpClientConfig::default()
        },
    )
    .unwrap();
    JwtVerifier::with_http_client(&config, client).unwrap()
}

fn user() -> Principal {
    Principal::new("u1", AuthMethod::Jwt).with_scopes(["orders:read"])
}

#[tokio::test]
async fn verifies_and_caches_keys() {
    let idp = Idp::default();
    let k1 = issuer("k1");
    idp.publish(Some(k1.jwks().unwrap()));
    let verifier = verifier(start(idp.clone()).await, 300, 30);

    for _ in 0..3 {
        let token = k1.issue(&user()).unwrap().access_token;
        let principal = verifier.verify(&token).await.unwrap();
        assert!(principal.has_scope("orders:read"));
    }
    assert_eq!(idp.hits(), 1);
}

#[tokio::test]
async fn picks_up_rotated_key() {
    let idp = Idp::default();
    let k1 = issuer("k1");
    let k2 = issuer("k2");
    idp.publish(Some(k1.jwks().unwrap()));
    let verifier = verifier(start(idp.clone()).await, 300, 0);
    verifier.preload().await.unwrap();

    let mut both = k1.jwks().unwrap();
    both.keys.extend(k2.jwks().unwrap().keys);
    idp.publish(Some(both));

    let token = k2.issue(&user()).unwrap().access_token;
    assert_eq!(verifier.verify(&token).await.unwrap().subject(), "u1");
    assert_eq!(idp.hits(), 2);
}

#[tokio::test]
async fn unknown_kid_refetches_at_most_once_per_interval() {
    let idp = Idp::default();
    let k1 = issuer("k1");
    let rogue = issuer("rogue");
    idp.publish(Some(k1.jwks().unwrap()));
    let verifier = verifier(start(idp.clone()).await, 300, 60);
    verifier.preload().await.unwrap();

    for _ in 0..5 {
        let token = rogue.issue(&user()).unwrap().access_token;
        let err = verifier.verify(&token).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unauthorized);
        assert_eq!(err.code(), "INVALID_TOKEN");
    }
    assert_eq!(idp.hits(), 1);
}

#[tokio::test]
async fn idp_outage() {
    let idp = Idp::default();
    let k1 = issuer("k1");
    let verifier = verifier(start(idp.clone()).await, 0, 0);
    let token = k1.issue(&user()).unwrap().access_token;

    let err = verifier.verify(&token).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Unavailable);
    assert_eq!(err.code(), "JWKS_UNAVAILABLE");

    idp.publish(Some(k1.jwks().unwrap()));
    assert!(verifier.verify(&token).await.is_ok());

    idp.publish(None);
    assert!(
        verifier.verify(&token).await.is_ok(),
        "stale keys keep serving while the IdP is down"
    );
}
