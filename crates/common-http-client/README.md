# common-http-client

A [reqwest](https://github.com/seanmonstar/reqwest) client for service-to-service calls. Part of [rust-common](../../README.md).

```rust
use common_http_client::HttpClient;

let payments = HttpClient::new("payments", &cfg.clients.payments)?;
let invoice: Invoice = payments.send_json(payments.get(format!("{base}/invoices/{id}"))).await?;
```

## Configuration

```toml
[clients.payments]
timeout_secs = 10
connect_timeout_secs = 2
retry = { max_attempts = 3, initial_delay_ms = 100 }
circuit_breaker = { failure_threshold = 5, open_secs = 30 }
```

## Behavior

- **Retries** apply only to idempotent methods (GET, HEAD, PUT, DELETE, OPTIONS, TRACE) and only for `Timeout`, `Unavailable` and `RateLimited` errors: 429, 502, 503, 504 and connection failures.
- **Circuit breaker** (optional, one per client) counts only infrastructure failures. Responses such as 404 or 409 do not open it.
- **Errors:** `send` returns `Err` for any status other than 2xx or 3xx. When the upstream responds with problem+json, its `code` and `detail` are kept, so a `USER_NOT_FOUND` from another service passes through this one unchanged.
- **Tracing:** every request runs in a client span. Query strings and credentials are removed from URLs before they reach logs or spans.
- **Propagation:** with the `otel` feature, a `traceparent` header is injected into each request.

## Features

| Feature | Enables |
|---|---|
| `otel` | W3C trace-context injection |
