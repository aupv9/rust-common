# common-config

Layered configuration loading with profiles and validation. Part of [rust-common](../../README.md).

```rust
use common_config::ConfigLoader;

let cfg: AppConfig = ConfigLoader::from_env("APP").load_validated()?;
```

`ConfigLoader::from_env("APP")` merges these sources, each one overriding the previous:

1. `config/default.toml` (optional)
2. `config/{APP_ENV}.toml` (optional), for example `config/production.toml` when `APP_ENV=production`
3. Environment variables prefixed with `APP__`, with `__` separating nested keys: `APP__SERVER__PORT=9000` sets `server.port`

`load()` deserializes into any `serde::Deserialize` type. `load_validated()` also runs the type's `Validate` implementation and returns an `ErrorKind::Config` error when validation fails.

For other layouts, compose the loader yourself:

```rust
let cfg: AppConfig = ConfigLoader::new()
    .required_file("/etc/my-service/config.toml")
    .toml_str(include_str!("defaults.toml"))
    .env("MY_SERVICE")
    .load()?;
```

Sensitive values should use `common_types::Secret` so they never show up in `Debug` output.

This crate does not depend on tokio.
