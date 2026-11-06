# Architecture

rust-common is a Cargo workspace of small crates arranged in tiers. A crate may depend only on crates in lower tiers, which keeps compile times down and lets a service pull in exactly what it uses.

```
Tier 3  Testing      common-test                 (dev-dependency only)
Tier 2  Adapters     common-axum
Tier 1  Infra        common-db   common-http-client ─▶ common-resilience
                     common-lifecycle   common-telemetry
                     common-auth ─▶ common-http-client (jwks feature)
Tier 0  Foundation   common-config ─▶ common-types ─▶ common-error

        Facade       common  (re-exports, gated by features)
```

## Principles

**Foundation stays runtime-free.** Tier 0 crates do not depend on tokio, so they can be used in CLIs, build scripts and libraries that are not async.

**One error type.** Every crate returns `common_error::Result`. Its `ErrorKind` is the single source of truth for HTTP status codes (`common-axum`), retry decisions (`common-resilience`, `common-http-client`) and circuit-breaker accounting. An error raised deep in the database layer reaches the client as a correct problem+json response without any mapping code in the service.

**Codes survive service boundaries.** `common-http-client` reads problem+json from upstream services and keeps their `code`, so an error such as `USER_NOT_FOUND` passes unchanged through a chain of services built on this library.

**Integrations are opt-in.** sqlx, reqwest, serde_json and OpenTelemetry sit behind feature flags. The `common` facade exposes the same flags, and CI checks every feature on its own with `cargo hack`.

**Shutdown is a first-class concern.** `common-lifecycle` owns the root `CancellationToken`. The HTTP server, background tasks and health checks all observe it, so a deploy drains traffic, finishes in-flight work and exits within a bounded time.

**One TLS stack.** reqwest and sqlx both use rustls with the aws-lc-rs provider, so only one crypto library is compiled and linked.

**Auth is framework-agnostic.** `common-auth` knows nothing about axum: it turns a credential into a `Principal` and checks requirements and policies. The axum glue (`with_auth`, `Auth`, `require`) lives in `common-axum` behind the `auth` feature, so the dependency still points down a tier.

## Request flow

```
client ──▶ x-request-id ──▶ trace span ──▶ panic catch ──▶ timeout ──▶ gzip ──▶ CORS ──▶ body limit ──▶ [auth] ──▶ handler
                                │                                                                          │
                     traceparent (otel)                                                     common_error::Error
                                                                                                          │
client ◀──────────────────────────── application/problem+json ◀──────── ApiError ◀───────────────────────┘
```

Outbound calls from a handler go through `HttpClient`: client span → `traceparent` injection → circuit breaker → retry with backoff → reqwest. An upstream problem+json response comes back as a `common_error::Error` with the upstream code preserved.
