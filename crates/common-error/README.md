# common-error

The shared error type for every crate in [rust-common](../../README.md).

```rust
use common_error::{Error, ErrorKind, OptionExt, Result, ResultExt};

fn load(path: &str) -> Result<String> {
    std::fs::read_to_string(path).context(ErrorKind::Config, format!("reading {path}"))
}

fn find(id: u64) -> Result<User> {
    repo_get(id).or_not_found(format!("user {id}"))
}

let err = Error::conflict("email already registered").with_code("EMAIL_TAKEN");
assert_eq!(err.kind(), ErrorKind::Conflict);
assert_eq!(err.code(), "EMAIL_TAKEN");
```

An `Error` has four parts:

| Part | Purpose |
|---|---|
| `kind` | One of the `ErrorKind` variants below; decides the HTTP status and whether a retry makes sense |
| `code` | Stable, machine-readable identifier such as `USER_NOT_FOUND`; defaults to the kind (`NOT_FOUND`) |
| `message` | Human-readable detail |
| `source` | Optional underlying error, kept for logs |

## Kinds

| `ErrorKind` | Code | `is_retryable()` |
|---|---|---|
| `NotFound` | `NOT_FOUND` | |
| `InvalidInput` | `INVALID_INPUT` | |
| `Unauthorized` | `UNAUTHORIZED` | |
| `Forbidden` | `FORBIDDEN` | |
| `Conflict` | `CONFLICT` | |
| `RateLimited` | `RATE_LIMITED` | ✓ |
| `Timeout` | `TIMEOUT` | ✓ |
| `Unavailable` | `UNAVAILABLE` | ✓ |
| `Config` | `CONFIG` | |
| `Internal` | `INTERNAL` | |

`ErrorKind::from_http_status` maps an upstream status back to a kind (400/422 → `InvalidInput`, 404/410 → `NotFound`, 409/412 → `Conflict`, 408/504 → `Timeout`, 502/503 → `Unavailable`, and so on).

## Conversions

| Feature | Conversion |
|---|---|
| *(always)* | `From<std::io::Error>` |
| `serde_json` | `From<serde_json::Error>` → `InvalidInput` for syntax/data errors |
| `reqwest` | `From<reqwest::Error>` → `Timeout`, `Unavailable` or the kind for the response status |
| `sqlx` | `From<sqlx::Error>`, mapped as below |

| sqlx error | `ErrorKind` |
|---|---|
| `RowNotFound` | `NotFound` |
| unique or foreign-key violation | `Conflict` |
| not-null or check violation | `InvalidInput` |
| `PoolTimedOut` | `Timeout` |
| `PoolClosed`, I/O, TLS | `Unavailable` |
| anything else | `Internal` |

This crate does not depend on tokio.
