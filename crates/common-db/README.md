# common-db

Postgres via [sqlx](https://github.com/launchbadge/sqlx): pool setup, health checks, migrations and transactions. Part of [rust-common](../../README.md).

```rust
use common_db::{connect, health_check, migrate, transaction};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

let pool = connect(&cfg.db).await?;
migrate(&pool, &MIGRATOR).await?;
lifecycle.health().register("db", health_check(pool.clone()));

let user_id = transaction(&pool, |tx| Box::pin(async move {
    let id = Id::<User>::new();
    sqlx::query("INSERT INTO users (id, name) VALUES ($1, $2)").bind(id).bind(&name).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO audit (user_id) VALUES ($1)").bind(id).execute(&mut *tx).await?;
    Ok(id)
})).await?;
```

`transaction` commits when the closure returns `Ok` and rolls back when it returns `Err`. `Id<T>`, `NonEmptyString` and `Timestamp` bind and decode directly. sqlx errors convert into `common_error::Error` as described in [common-error](../common-error#conversions).

## Configuration

```toml
[db]
url = "postgres://app:secret@db:5432/app"
max_connections = 20
min_connections = 0
acquire_timeout_secs = 5
statement_timeout_ms = 5000
application_name = "user-service"
```

`url` is a `Secret` and never appears in logs. `statement_timeout_ms` and `application_name` are set on every connection. `connect` verifies connectivity at startup; `connect_lazy` defers it to the first query.

TLS uses rustls with the aws-lc-rs provider, the same one `common-http-client` uses.
