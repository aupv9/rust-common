# common-axum

An opinionated [axum](https://github.com/tokio-rs/axum) stack. Part of [rust-common](../../README.md).

```rust
use common_axum::{ApiResult, Json, Path, ValidatedJson, serve};

async fn create_user(ValidatedJson(input): ValidatedJson<CreateUser>) -> ApiResult<Json<User>> {
    Ok(Json(repo.insert(input).await?))
}

serve(router, &cfg.server, lifecycle).await?;
```

`serve` applies the middleware stack, mounts the health routes, binds the listener and shuts down when the `Lifecycle` does. Use `app()` to build the same `Router` without serving it, for example in tests.

## Configuration

```toml
[server]
host = "0.0.0.0"
port = 8080
request_timeout_secs = 30
body_limit_bytes = 2097152
cors.allowed_origins = ["https://app.example.com"]
```

## Middleware, outermost first

1. `x-request-id`: generates a UUID v7 when the client does not send one and echoes it in the response
2. `http.request` span with method, matched route, status and request id; with `otel`, the span continues the trace from the incoming `traceparent` header
3. Panic recovery → 500 problem+json
4. Request timeout → 504 problem+json
5. gzip compression
6. CORS, only when `cors.allowed_origins` is non-empty; `["*"]` allows any origin
7. Request body size limit

Unknown routes return 404 and wrong methods return 405, both as problem+json. `/health/live` and `/health/ready` sit outside the middleware so probes do not flood the logs. Note that `with_defaults` replaces the router's `fallback`.

## Errors

Handlers return `ApiResult<T>`. Any `common_error::Error` converts into `ApiError`, which renders an [RFC 9457](https://www.rfc-editor.org/rfc/rfc9457) response:

```json
{"type":"about:blank","title":"Not Found","status":404,"detail":"user 42","code":"USER_NOT_FOUND"}
```

| `ErrorKind` | Status |
|---|---|
| `NotFound` | 404 |
| `InvalidInput` | 400 |
| `Unauthorized` / `Forbidden` | 401 / 403 |
| `Conflict` | 409 |
| `RateLimited` | 429 |
| `Timeout` | 504 |
| `Unavailable` | 503 |
| `Internal`, `Config` | 500 |

For 500 responses the `detail` is replaced by a generic message and the original error is logged with the request span.

## Extractors

`Json`, `Query` and `Path` wrap the axum extractors so rejections are problem+json too. They keep axum's status codes (415 for a missing content type, 422 for a body with the wrong shape, and so on). `ValidatedJson<T>` also runs `T::validate()` and returns 400 when it fails.

## Features

| Feature | Enables |
|---|---|
| `otel` | Continues incoming W3C trace context |
