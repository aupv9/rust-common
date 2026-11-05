# common

Facade crate for [rust-common](../../README.md). It re-exports every `common-*` crate behind a feature flag, so a service needs a single dependency.

```toml
[dependencies]
common = { git = "https://github.com/aupv9/rust-common", branch = "main", features = ["config", "telemetry", "axum", "db", "otel"] }
```

| Module | Crate | Feature |
|---|---|---|
| `common::error` and root re-exports | [`common-error`](../common-error) | always |
| `common::config` | [`common-config`](../common-config) | `config` |
| `common::telemetry` | [`common-telemetry`](../common-telemetry) | `telemetry` |
| `common::types` | [`common-types`](../common-types) | `types` |
| `common::lifecycle` | [`common-lifecycle`](../common-lifecycle) | `lifecycle` |
| `common::axum` | [`common-axum`](../common-axum) | `axum` |
| `common::resilience` | [`common-resilience`](../common-resilience) | `resilience` |
| `common::db` | [`common-db`](../common-db) | `db` |
| `common::http_client` | [`common-http-client`](../common-http-client) | `http-client` |

Cross-cutting features: `otel`, `sqlx`, `reqwest`, `serde_json`, and `full` for everything. See the [feature table](../../README.md#feature-flags) in the root README.

## Examples

```sh
cargo run -p common --example basic --features config,telemetry,types
cargo run -p common --example service --features full
```

`service` starts an HTTP server on `0.0.0.0:8080` with `/users` endpoints, a background task and health checks.
