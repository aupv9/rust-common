# rust-common

Bộ thư viện dùng chung cho các backend service Rust (axum + Postgres/sqlx), tổ chức dạng Cargo workspace.

MSRV: **1.94**

## Kiến trúc

Các crate chia theo tầng, phụ thuộc chỉ đi từ trên xuống. Tier 0 không dùng tokio.

```
Tier 2  Adapters    common-axum                                  (phase 2)
Tier 1  Infra       common-lifecycle  common-db  common-http-client  common-telemetry
Tier 0  Foundation  common-config ── common-types ── common-error
        Facade      common  (re-export, bật/tắt bằng feature)
```

| Crate | Trạng thái | Nội dung |
|---|---|---|
| `common-error` | ✅ | `Error` (kind + code + message + source), `ResultExt::context`, `OptionExt::or_not_found`, `From` cho `io` / `serde_json` / `sqlx` |
| `common-types` | ✅ | `Id<T>` (UUID v7 có kiểu), `Secret<T>`, `NonEmptyString`, `PageRequest`/`Page<T>`, `CursorRequest`/`CursorPage<T>`; hỗ trợ sqlx Postgres qua feature |
| `common-config` | ✅ | `ConfigLoader`: `config/default.toml` → `config/{APP_ENV}.toml` → env `APP__*`, trait `Validate` |
| `common-telemetry` | ✅ | khởi tạo tracing (pretty / compact / json) |

## Features của crate `common`

| Feature | Bật |
|---|---|
| *(luôn có)* | `common::error`, `Error`, `ErrorKind`, `Result`, `ResultExt`, `OptionExt` |
| `config` | `common::config` |
| `telemetry` | `common::telemetry` |
| `types` | `common::types` |
| `serde_json` | `From<serde_json::Error> for Error` |
| `sqlx` | `From<sqlx::Error> for Error`; `Id<T>`, `NonEmptyString` encode/decode Postgres |
| `full` | tất cả |

## Sử dụng

```toml
[dependencies]
common = { git = "<repo-url>", tag = "v0.1.0", features = ["config", "telemetry", "types", "sqlx"] }
```

```rust
use common::config::{ConfigLoader, Validate};
use common::types::{Id, Page, PageRequest};
use common::{ErrorKind, OptionExt, ResultExt};

let cfg: AppConfig = ConfigLoader::from_env("APP").load_validated()?;

let user = sqlx::query_as::<_, User>("select * from users where id = $1")
    .bind(user_id)
    .fetch_optional(&pool)
    .await?
    .or_not_found(format!("user {user_id}"))?;

let body = std::fs::read("x").context(ErrorKind::Internal, "read x")?;
```

### Config theo profile

```
config/
├── default.toml   # luôn load
├── local.toml     # APP_ENV không đặt → "local"
└── prod.toml      # APP_ENV=prod
```

Biến môi trường ghi đè cuối cùng: `APP__DB__URL=...` → `db.url`.

### Mapping lỗi sqlx

| sqlx | ErrorKind |
|---|---|
| `RowNotFound` | `NotFound` |
| unique / foreign key violation | `Conflict` |
| not-null / check violation | `InvalidInput` |
| `PoolTimedOut` | `Timeout` |
| `PoolClosed`, I/O, TLS | `Unavailable` |
| khác | `Internal` |

Message trả ra luôn là chuỗi chung chung (không lộ chi tiết DB); lỗi gốc nằm trong `source()` để log.

Ví dụ đầy đủ: `cargo run -p common --example basic --features full`

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
