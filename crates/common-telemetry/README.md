# common-telemetry

`tracing` initialization with optional OpenTelemetry export. Part of [rust-common](../../README.md).

```rust
use common_telemetry::{ServiceInfo, TelemetryConfig, init};

let _guard = init(
    &ServiceInfo::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
    &cfg.telemetry,
)?;
```

Keep the returned guard alive for the whole process. Dropping it flushes pending spans and metrics.

## Configuration

```toml
[telemetry]
level = "info,sqlx=warn"
format = "json"

[telemetry.otlp]
endpoint = "http://otel-collector:4317"
sample_ratio = 0.1
metrics = true
```

| Key | Default | Description |
|---|---|---|
| `level` | `info` | `EnvFilter` directive. `RUST_LOG`, when set, overrides it |
| `format` | `compact` | `pretty`, `compact` or `json` |
| `otlp.endpoint` | `http://localhost:4317` | OTLP gRPC endpoint. Without a `[telemetry.otlp]` section nothing is exported |
| `otlp.sample_ratio` | `1.0` | Parent-based trace sampling ratio |
| `otlp.metrics` | `true` | Also export metrics over OTLP |

## Features

| Feature | Enables |
|---|---|
| `otel` | OTLP export of traces and metrics, and `propagation::{set_parent_from_headers, inject_headers}` for W3C `traceparent` headers |

With `otel` enabled, `common-axum` continues incoming traces and `common-http-client` propagates them to upstream services automatically.
