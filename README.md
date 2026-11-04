# rust-common

Bộ thư viện dùng chung (common library) cho các project Rust, tổ chức dạng Cargo workspace.

## Cấu trúc

```
rust-common/
├── Cargo.toml              # workspace: deps, lints, profile dùng chung
├── rust-toolchain.toml     # stable + rustfmt + clippy
├── rustfmt.toml
├── .gitlab-ci.yml          # fmt → clippy → test
└── crates/
    ├── common/             # facade: re-export các crate dưới qua feature flags
    ├── common-error/       # Error / ErrorKind / Result dùng chung
    ├── common-config/      # ConfigLoader: TOML/YAML file + inline + env (layered)
    ├── common-telemetry/   # khởi tạo tracing (pretty / compact / json)
    └── common-utils/       # helper nhỏ: id (UUID v7), time
```

## Sử dụng

Qua facade (khuyến nghị):

```toml
[dependencies]
common = { git = "<repo-url>", features = ["config", "telemetry"] }  # hoặc "full"
```

Hoặc dùng trực tiếp từng crate:

```toml
common-error = { git = "<repo-url>" }
```

```rust
use common::config::ConfigLoader;

let cfg: AppConfig = ConfigLoader::new()
    .file("config/default.toml")
    .file("config/local.toml")  // optional
    .env("APP")                 // APP__DB__URL -> db.url
    .load()?;
common::telemetry::init(&cfg.telemetry)?;
```

Xem ví dụ đầy đủ: `cargo run -p common --example basic --features full`

## Thêm crate mới

1. `cargo new --lib crates/common-<tên>`
2. Trong `Cargo.toml` của crate: dùng `version.workspace = true`, ... và `[lints] workspace = true`
3. Thêm vào `[workspace.dependencies]` ở root
4. (Tuỳ chọn) thêm feature + `pub use` trong `crates/common`

## Phát triển

```sh
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```
