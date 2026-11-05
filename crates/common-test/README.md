# common-test

Test helpers for services built with [rust-common](../../README.md). Use it as a dev-dependency.

```toml
[dev-dependencies]
common-test = { git = "https://github.com/aupv9/rust-common", branch = "main" }
```

```rust
use common_test::{TestApp, TestPostgres, init_tracing};

#[tokio::test]
#[ignore = "requires docker"]
async fn rejects_empty_name() {
    init_tracing();
    let pg = TestPostgres::start().await.unwrap();
    let app = TestApp::new(build_router(pg.pool().clone()));

    app.post_json("/users", &serde_json::json!({ "name": "" }))
        .await
        .assert_problem(StatusCode::BAD_REQUEST, "INVALID_INPUT");
}
```

| Item | Description |
|---|---|
| `init_tracing()` | Installs a test-friendly subscriber once per process; safe to call from every test |
| `TestPostgres` | Starts a disposable Postgres container through testcontainers and exposes `url()` and `pool()` |
| `TestApp` | Sends requests to an axum `Router` in process, without opening a port |
| `TestResponse` | `assert_status`, `assert_problem`, `json::<T>()`, `text()` |

## Features

| Feature | Default | Enables |
|---|---|---|
| `http` | ✓ | `TestApp`, `TestResponse` |
| `postgres` | ✓ | `TestPostgres` |

## Running Docker-backed tests

Mark tests that need Docker with `#[ignore]` and run them with `--include-ignored`. With [colima](https://github.com/abiosoft/colima):

```sh
export DOCKER_HOST=unix://$HOME/.colima/default/docker.sock
export TESTCONTAINERS_DOCKER_SOCKET_OVERRIDE=/var/run/docker.sock
cargo test --workspace --all-features -- --include-ignored
```
