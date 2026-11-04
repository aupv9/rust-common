# rust-common

Bộ thư viện dùng chung cho các backend service Rust (axum + Postgres/sqlx), tổ chức dạng Cargo workspace.

MSRV: **1.94**

## Kiến trúc

Các crate chia theo tầng, phụ thuộc chỉ đi từ trên xuống. Tier 0 không dùng tokio.

```
Tier 2  Adapters    common-axum
Tier 1  Infra       common-lifecycle   common-telemetry   (phase 3: common-db, common-http-client, ...)
Tier 0  Foundation  common-config ── common-types ── common-error
        Facade      common  (re-export, bật/tắt bằng feature)
```

| Crate | Nội dung |
|---|---|
| `common-error` | `Error` (kind + code + message + source), `ResultExt::context`, `OptionExt::or_not_found`, `From` cho `io` / `serde_json` / `sqlx` |
| `common-types` | `Id<T>` (UUID v7 có kiểu), `Timestamp` (chrono, độ chính xác micro giây khớp Postgres), `Secret<T>`, `NonEmptyString`, `PageRequest`/`Page<T>`, `CursorRequest`/`CursorPage<T>`, trait `Validate` |
| `common-config` | `ConfigLoader`: `config/default.toml` → `config/{APP_ENV}.toml` → env `APP__*` |
| `common-telemetry` | tracing (pretty / compact / json); feature `otel`: export trace + metric qua OTLP gRPC, W3C trace context |
| `common-lifecycle` | `Lifecycle`: tắt êm khi nhận SIGINT/SIGTERM, task nền có giám sát, `HealthRegistry` |
| `common-axum` | `ApiError` → `application/problem+json`, extractor `Json`/`Query`/`Path`/`ValidatedJson`, middleware chuẩn, `/health/*`, `serve()` |

## Features của crate `common`

| Feature | Bật |
|---|---|
| *(luôn có)* | `common::error`, `Error`, `ErrorKind`, `Result`, `ResultExt`, `OptionExt` |
| `config` | `common::config` |
| `telemetry` | `common::telemetry` |
| `types` | `common::types` |
| `lifecycle` | `common::lifecycle` |
| `axum` | `common::axum` (kéo theo `lifecycle`, `types`) |
| `otel` | OpenTelemetry trong `telemetry` và lan truyền trace context trong `axum` |
| `serde_json` | `From<serde_json::Error> for Error` |
| `sqlx` | `From<sqlx::Error> for Error`; `Id<T>`, `NonEmptyString` encode/decode Postgres; chrono cho sqlx |
| `full` | tất cả |

## Dựng một service

```toml
[dependencies]
common = { git = "<repo-url>", tag = "v0.2.0", features = ["config", "telemetry", "axum", "otel", "sqlx"] }
```

```rust
#[tokio::main]
async fn main() -> common::Result<()> {
    let cfg: AppConfig = ConfigLoader::from_env("APP").load_validated()?;
    let _telemetry = telemetry::init(
        &ServiceInfo::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
        &cfg.telemetry,
    )?;

    let lifecycle = Lifecycle::new(&cfg.lifecycle);
    lifecycle.health().register("db", move || { /* select 1 */ });
    lifecycle.spawn("outbox", |token| async move { /* loop until token.cancelled() */ Ok(()) });

    common::axum::serve(routes(), &cfg.server, lifecycle).await
}

async fn get_user(Path(id): Path<Id<User>>) -> ApiResult<Json<User>> {
    let user = repo.find(id).await?.or_not_found(format!("user {id}"))?;
    Ok(Json(user))
}
```

Ví dụ chạy được: `cargo run -p common --example service --features full`

### Config mẫu

```toml
[server]
host = "0.0.0.0"
port = 8080
request_timeout_secs = 30
body_limit_bytes = 2097152
cors.allowed_origins = ["https://app.example.com"]

[lifecycle]
shutdown_timeout_secs = 30

[telemetry]
level = "info,sqlx=warn"
format = "json"

[telemetry.otlp]
endpoint = "http://otel-collector:4317"
sample_ratio = 0.1
metrics = true
```

`RUST_LOG`, nếu được đặt, ghi đè `telemetry.level`. Không có mục `[telemetry.otlp]` thì không export.

### Middleware của `common-axum` (ngoài vào trong)

1. `x-request-id`: tạo UUID v7 nếu client không gửi, trả lại trong response
2. Trace span `http.request` (method, route, status, request_id); với `otel` thì nối vào trace cha từ header `traceparent`
3. Bắt panic → 500 problem+json
4. Timeout → 504 problem+json
5. Nén gzip
6. CORS (chỉ bật khi `cors.allowed_origins` khác rỗng; `["*"]` cho phép mọi origin)
7. Giới hạn kích thước body

Route không tồn tại trả 404, method sai trả 405, đều ở dạng problem+json. `/health/live` và `/health/ready` nằm ngoài middleware để probe không làm nhiễu log. Lưu ý: `with_defaults` đặt lại `fallback` của router.

### Mapping lỗi → HTTP

| ErrorKind | Status |
|---|---|
| `NotFound` | 404 |
| `InvalidInput` | 400 |
| `Unauthorized` / `Forbidden` | 401 / 403 |
| `Conflict` | 409 |
| `RateLimited` | 429 |
| `Timeout` | 504 |
| `Unavailable` | 503 |
| `Internal`, `Config` | 500 (`detail` bị ẩn, lỗi gốc được log) |

```json
{"type":"about:blank","title":"Not Found","status":404,"detail":"user 42","code":"USER_NOT_FOUND"}
```

Lỗi từ extractor giữ status gốc của axum (415 thiếu content-type, 422 sai kiểu dữ liệu, ...).

### Mapping lỗi sqlx

| sqlx | ErrorKind |
|---|---|
| `RowNotFound` | `NotFound` |
| unique / foreign key violation | `Conflict` |
| not-null / check violation | `InvalidInput` |
| `PoolTimedOut` | `Timeout` |
| `PoolClosed`, I/O, TLS | `Unavailable` |
| khác | `Internal` |

### Lifecycle

- Task được `spawn` nhận một `CancellationToken` và phải tự dừng khi token bị huỷ.
- Task trả `Err` hoặc panic thì cả app bắt đầu tắt, và `run()` trả về lỗi đó.
- Khi đang tắt, `/health/ready` trả 503 với `status: "draining"` để load balancer ngừng gửi traffic.
- Quá `shutdown_timeout_secs` mà task chưa dừng thì `run()` trả lỗi `Timeout`.

## Phát triển

```sh
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check
cargo hack check --workspace --each-feature --no-dev-deps
```

Quy ước: không viết comment trong code; giải thích đặt ở README.

## Thêm crate mới

1. `cargo new --lib crates/common-<tên>`
2. `Cargo.toml`: `version.workspace = true`, ... và `[lints] workspace = true`
3. Thêm vào `[workspace.dependencies]` ở root
4. Thêm feature + `pub use` trong `crates/common`
