<div align="center">

# rust-common

**Production-ready building blocks for Rust backend services built on axum, Postgres and OpenTelemetry.**

[![CI](https://github.com/aupv9/rust-common/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/aupv9/rust-common/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![MSRV 1.94](https://img.shields.io/badge/MSRV-1.94-orange.svg)](https://blog.rust-lang.org/)
[![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)](https://github.com/rust-secure-code/safety-dance/)

[Features](#features) ·
[Quick start](#quick-start) ·
[Crates](#crates) ·
[Feature flags](#feature-flags) ·
[Architecture](docs/architecture.md) ·
[Contributing](CONTRIBUTING.md)

</div>

---

`rust-common` is a Cargo workspace of small, focused crates that cover the code every HTTP service ends up writing: error handling, configuration, structured logging and tracing, graceful shutdown, health checks, an opinionated axum stack, a Postgres pool, a resilient HTTP client, and test helpers.

You can depend on the individual `common-*` crates, or on the `common` facade and turn on only the parts you need with feature flags.

## Features

- **One error type end to end.** `common::Error` carries a kind, a stable machine-readable code, a message and a source. It maps to HTTP status codes and to [RFC 9457](https://www.rfc-editor.org/rfc/rfc9457) `application/problem+json` responses, and is built from `sqlx`, `reqwest`, `serde_json` and `io` errors.
- **An axum stack with defaults.** Request IDs, tracing spans, panic recovery, timeouts, gzip, CORS, body limits, `/health/live` and `/health/ready`, and problem+json for every error, including 404, 405 and extractor rejections.
- **Graceful lifecycle.** Shuts down on SIGINT/SIGTERM, supervises background tasks (a failing or panicking task stops the service), and reports a draining state so load balancers stop sending traffic first.
- **Observability.** `tracing` output in pretty, compact or JSON format. Behind the `otel` feature: OTLP gRPC export of traces and metrics, and W3C trace-context propagation on incoming and outgoing requests.
- **Authentication and authorization.** JWT verification against an IdP's JWKS (cached, rotation-aware, outage-tolerant) or static keys, JWT issuing, hashed API keys, Argon2id passwords, and a `Principal` with roles, scopes and an optional tenant. Role/scope checks as axum layers, resource policies in handlers.
- **Postgres via sqlx.** Pool setup, statement timeouts, health checks, migrations, and a `transaction` helper that commits on `Ok` and rolls back on `Err`.
- **Resilient outbound HTTP.** Timeouts, retries with exponential backoff and jitter (idempotent methods only), a circuit breaker per client, and problem+json errors from upstream services passed through with their code intact.
- **Typed domain primitives.** `Id<T>` (UUID v7), `Secret<T>`, `NonEmptyString`, offset and cursor pagination, and microsecond `Timestamp`s that round-trip through Postgres.
- **Test helpers.** Throwaway Postgres via testcontainers and in-process axum requests with problem+json assertions.
- **Safe and lean.** `unsafe_code` forbidden across the workspace, clippy pedantic, `cargo-deny` checks, every feature combination checked in CI, and a rustls-only TLS stack with aws-lc-rs.

## Quick start

Add the facade with the features you need:

```toml
[dependencies]
common = { git = "https://github.com/aupv9/rust-common", branch = "main", features = ["config", "telemetry", "axum"] }
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

> [!NOTE]
> The crates are not yet published to crates.io. Pin a `rev` (or a `tag`, once releases are cut) for reproducible builds.

Write a service:

```rust
use common::axum::axum::{Router, routing::get};
use common::axum::{ApiResult, Json, Path, ServerConfig};
use common::config::ConfigLoader;
use common::lifecycle::{Lifecycle, LifecycleConfig};
use common::telemetry::{self, ServiceInfo, TelemetryConfig};
use common::types::{Id, NonEmptyString};
use common::OptionExt;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct AppConfig {
    server: ServerConfig,
    telemetry: TelemetryConfig,
    lifecycle: LifecycleConfig,
}

#[derive(Clone, Serialize)]
struct User {
    id: Id<User>,
    name: NonEmptyString,
}

async fn find_user(_id: Id<User>) -> common::Result<Option<User>> {
    Ok(None)
}

async fn get_user(Path(id): Path<Id<User>>) -> ApiResult<Json<User>> {
    let user = find_user(id).await?.or_not_found(format!("user {id}"))?;
    Ok(Json(user))
}

#[tokio::main]
async fn main() -> common::Result<()> {
    let cfg: AppConfig = ConfigLoader::from_env("APP").load()?;
    let _telemetry = telemetry::init(
        &ServiceInfo::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
        &cfg.telemetry,
    )?;

    let lifecycle = Lifecycle::new(&cfg.lifecycle);
    let router = Router::new().route("/users/{id}", get(get_user));

    common::axum::serve(router, &cfg.server, lifecycle).await
}
```

A missing user returns:

```http
HTTP/1.1 404 Not Found
Content-Type: application/problem+json
x-request-id: 0192f6a8-...

{"type":"about:blank","title":"Not Found","status":404,"detail":"user 0192f6a8-...","code":"NOT_FOUND"}
```

A complete example with state, validation, pagination, a background task and a health check is in [`crates/common/examples/service.rs`](crates/common/examples/service.rs):

```sh
cargo run -p common --example service --features full
curl -s localhost:8080/health/ready
```

## Crates

| Crate | Description |
|---|---|
| [`common`](crates/common) | Facade that re-exports every crate below behind feature flags |
| [`common-error`](crates/common-error) | `Error`, `ErrorKind`, `ResultExt`, `OptionExt`; conversions from `io`, `serde_json`, `sqlx`, `reqwest` |
| [`common-types`](crates/common-types) | `Id<T>`, `Timestamp`, `Secret<T>`, `NonEmptyString`, offset and cursor pagination, `Validate` |
| [`common-config`](crates/common-config) | Layered config: `config/default.toml` → `config/{APP_ENV}.toml` → `APP__*` environment variables |
| [`common-telemetry`](crates/common-telemetry) | `tracing` setup; optional OTLP traces and metrics and W3C propagation |
| [`common-lifecycle`](crates/common-lifecycle) | Graceful shutdown, supervised background tasks, `HealthRegistry` |
| [`common-axum`](crates/common-axum) | problem+json errors, extractors, middleware stack, health routes, `serve()` |
| [`common-resilience`](crates/common-resilience) | `retry` / `retry_if` with backoff and jitter, `CircuitBreaker` |
| [`common-db`](crates/common-db) | Postgres pool, `health_check`, `migrate`, `transaction` |
| [`common-http-client`](crates/common-http-client) | `reqwest` client with timeouts, retries, circuit breaker, tracing, upstream error mapping |
| [`common-auth`](crates/common-auth) | JWT verification (JWKS or static keys), JWT issuing, API keys, password hashing, `Principal`, RBAC/scope and resource policies |
| [`common-test`](crates/common-test) | `init_tracing`, `TestPostgres`, `TestApp` / `TestResponse` |

Each crate's README covers its configuration and behavior in detail. The layering rules are described in [docs/architecture.md](docs/architecture.md).

## Feature flags

The `common` facade enables nothing beyond the error types by default.

| Feature | Enables |
|---|---|
| *(always)* | `common::error`, `Error`, `ErrorKind`, `Result`, `ResultExt`, `OptionExt` |
| `config` | `common::config` |
| `telemetry` | `common::telemetry` |
| `types` | `common::types` |
| `lifecycle` | `common::lifecycle` |
| `axum` | `common::axum` (implies `lifecycle`, `types`) |
| `resilience` | `common::resilience` |
| `db` | `common::db` (implies `types`, `sqlx`) |
| `http-client` | `common::http_client` (implies `resilience`, `reqwest`) |
| `auth` | `common::auth` with static-key JWT and API keys; `with_auth`/`Auth`/`require` in `axum` (implies `types`) |
| `auth-jwks` | JWKS key source (implies `auth`, `http-client`) |
| `auth-issuer` | `JwtIssuer` (implies `auth`) |
| `auth-password` | Argon2id password hashing (implies `auth`) |
| `otel` | OpenTelemetry export in `telemetry`; trace-context extraction in `axum` and injection in `http-client` |
| `sqlx` | `From<sqlx::Error>`; Postgres `Encode`/`Decode` for `Id<T>` and `NonEmptyString` |
| `reqwest` | `From<reqwest::Error>` |
| `serde_json` | `From<serde_json::Error>` |
| `full` | Everything above |

## Error model

| `ErrorKind` | HTTP status | Retryable |
|---|---|---|
| `NotFound` | 404 | |
| `InvalidInput` | 400 | |
| `Unauthorized` / `Forbidden` | 401 / 403 | |
| `Conflict` | 409 | |
| `RateLimited` | 429 | ✓ |
| `Timeout` | 504 | ✓ |
| `Unavailable` | 503 | ✓ |
| `Internal`, `Config` | 500 (detail hidden, source logged) | |

The full mapping, including how `sqlx` errors become kinds, is in [`common-error`](crates/common-error) and [`common-axum`](crates/common-axum).

## Minimum supported Rust version

The MSRV is **1.94** (edition 2024), required by sqlx 0.9. CI checks it on every change. Raising the MSRV is not treated as a breaking change, but it is always noted in the [changelog](CHANGELOG.md).

## Roadmap

- [x] Foundation: errors, types, configuration
- [x] Service kit: lifecycle, axum, telemetry, OpenTelemetry
- [x] Data and integration: Postgres, HTTP client, resilience, test helpers
- [x] Authentication and authorization
- [ ] HTTP server metrics
- [ ] On demand: caching, messaging, gRPC

Ideas and requests are welcome in [issues](https://github.com/aupv9/rust-common/issues).

## Contributing

Contributions are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) for the development setup and conventions, and follow the [Code of Conduct](CODE_OF_CONDUCT.md). Report security issues as described in [SECURITY.md](SECURITY.md), not in public issues.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
