# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Until 1.0, minor versions may contain breaking changes.

## [Unreleased]

### Added

- **common-error**: `Error` with kind, code, message and source; `ResultExt` and `OptionExt`; retryable kinds; `ErrorKind::from_http_status`; conversions from `io`, `serde_json`, `sqlx` and `reqwest` behind feature flags.
- **common-types**: `Id<T>` (UUID v7), `Timestamp` truncated to microseconds, `Secret<T>`, `NonEmptyString`, offset and cursor pagination, `Validate`, and sqlx Postgres support.
- **common-config**: layered loading (`default` → `APP_ENV` profile → environment) with validation.
- **common-telemetry**: `tracing` initialization with a flush guard; `otel` feature for OTLP gRPC export of traces and metrics and W3C trace-context propagation.
- **common-lifecycle**: signal-driven graceful shutdown, supervised background tasks, `HealthRegistry` with per-check timeouts and a draining state.
- **common-axum**: problem+json errors, `Json`/`Query`/`Path`/`ValidatedJson` extractors, default middleware stack, health routes and `serve()`.
- **common-resilience**: `retry`/`retry_if` with exponential backoff and jitter; `CircuitBreaker` that counts only infrastructure failures.
- **common-db**: `DbConfig`, `connect`/`connect_lazy`, `ping`, `health_check`, `migrate` and `transaction` for Postgres.
- **common-http-client**: reqwest client with timeouts, retries for idempotent methods, circuit breaker, client spans, URL redaction, upstream problem+json propagation and optional `traceparent` injection.
- **common-test**: `init_tracing`, `TestPostgres` and `TestApp`/`TestResponse`.
- **common-auth**: `Principal` with subject, optional tenant, roles, scopes and claims; JWT verification with static keys or a cached JWKS (single-flight refresh, rate-limited refetch on unknown `kid`, stale keys during IdP outages); configurable claim mapping with JSON pointers; hashed API keys; `Requirement` (role/scope combinators) and `Policy<R>` for resource checks; `issuer` feature for signing JWTs and publishing a JWKS; `password` feature for Argon2id.
- **common-axum**: `auth` feature with `with_auth` middleware, `Auth` / `Option<Auth>` extractors and the `require` route layer; request spans record `enduser.id`.
- **common-axum**: with `otel`, the default stack records `http.server.request.duration` and `http.server.active_requests` following the OpenTelemetry HTTP semantic conventions; `HttpMetricsLayer` for routers built without `with_defaults`.
- **common**: facade crate with feature flags for every crate above.

### Changed

- `common-utils` was replaced by `common-types`.
- MSRV raised to 1.94, required by sqlx 0.9.

### Security

- Bumped `webpki-roots` and `webpki-root-certs` to 1.0.11 ([RUSTSEC-2026-0381](https://rustsec.org/advisories/RUSTSEC-2026-0381), [RUSTSEC-2026-0383](https://rustsec.org/advisories/RUSTSEC-2026-0383)).

[Unreleased]: https://github.com/aupv9/rust-common/commits/main
