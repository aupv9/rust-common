# common-types

Domain primitives shared across services. Part of [rust-common](../../README.md).

| Type | Description |
|---|---|
| `Id<T>` | UUID v7 tagged with the entity type, so `Id<User>` and `Id<Order>` cannot be mixed up. Time-ordered, which keeps B-tree indexes compact |
| `Timestamp` | `chrono::DateTime<Utc>` truncated to microseconds to match Postgres `timestamptz`, so values round-trip exactly. `time::now()` returns one |
| `Secret<T>` | Wraps credentials; `Debug` prints `Secret(***)`. Read the value with `expose()` |
| `NonEmptyString` | A string that fails deserialization when empty or whitespace-only |
| `PageRequest` / `Page<T>` | Offset pagination (`page`, `per_page`, clamped to a maximum) with `limit()`/`offset()` for SQL |
| `CursorRequest` / `CursorPage<T>` | Cursor pagination with `next_cursor` |
| `Validate` | Trait for semantic validation after deserialization; used by `ValidatedJson` and `ConfigLoader::load_validated` |

```rust
use common_types::{Id, NonEmptyString, Secret, Timestamp, Validate, time};

struct User;

#[derive(serde::Deserialize)]
struct Signup {
    name: NonEmptyString,
    password: Secret<String>,
}

impl Validate for Signup {
    fn validate(&self) -> common_error::Result<()> {
        if self.password.expose().len() < 12 {
            return Err(common_error::Error::invalid_input("password too short"));
        }
        Ok(())
    }
}

let id: Id<User> = Id::new();
let created_at: Timestamp = time::now();
```

## Features

| Feature | Enables |
|---|---|
| `sqlx` | Postgres `Type`/`Encode`/`Decode` for `Id<T>` and `NonEmptyString`, and chrono support in sqlx |

This crate does not depend on tokio.
