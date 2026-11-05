# common-resilience

Retries with backoff and a circuit breaker. Part of [rust-common](../../README.md).

## Retry

```rust
use common_resilience::{RetryPolicy, retry};

let policy = RetryPolicy { max_attempts: 5, ..RetryPolicy::default() };
let value = retry(&policy, |attempt| async move { fetch(attempt).await }).await?;
```

`retry` retries only errors whose kind is retryable (`Timeout`, `Unavailable`, `RateLimited`). `retry_if` takes a custom predicate and works with any error type.

| Field | Default | Description |
|---|---|---|
| `max_attempts` | 3 | Total attempts, including the first |
| `initial_delay_ms` | 100 | Delay before the first retry |
| `max_delay_ms` | 5000 | Upper bound for any delay |
| `multiplier` | 2.0 | Exponential growth factor |
| `jitter` | `true` | Randomizes each delay to 50–100% of its value |

## Circuit breaker

```rust
use common_resilience::{CircuitBreaker, CircuitBreakerConfig};

let breaker = CircuitBreaker::new("payments", &CircuitBreakerConfig { failure_threshold: 5, open_secs: 30 });
let invoice = breaker.call(client.fetch_invoice(id)).await?;
```

After `failure_threshold` consecutive failures the breaker opens and rejects calls immediately with `Unavailable` for `open_secs`. It then lets one trial call through (half-open) and closes again if that call succeeds.

Only infrastructure failures (retryable kinds) count toward the threshold. Business errors such as `NotFound` or `Conflict` mean the dependency is healthy, so they never open the circuit.
