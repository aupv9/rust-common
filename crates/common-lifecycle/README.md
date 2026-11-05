# common-lifecycle

Graceful shutdown, background task supervision and health checks. Part of [rust-common](../../README.md).

```rust
use common_lifecycle::{Lifecycle, LifecycleConfig};

let lifecycle = Lifecycle::new(&LifecycleConfig::default());

lifecycle.health().register("db", move || async move { Ok(()) });

lifecycle.spawn("outbox-relay", |token| async move {
    loop {
        tokio::select! {
            () = token.cancelled() => break,
            () = relay_batch() => {}
        }
    }
    Ok(())
});

lifecycle.run().await?;
```

## Behavior

- `run()` waits for SIGINT or SIGTERM (or for `shutdown()` to be called), cancels the shared `CancellationToken` and waits for every spawned task to finish.
- Each task gets a child token and must stop when it is cancelled.
- A task that returns `Err` or panics starts the shutdown of the whole service, and `run()` returns that error. A service never keeps running with a dead worker.
- While shutting down, `HealthRegistry` reports `draining`, so `/health/ready` returns 503 and load balancers stop sending traffic.
- Tasks still running after `shutdown_timeout_secs` (default 30) are abandoned, and `run()` returns a `Timeout` error.
- Each health check has its own timeout. A check that hangs counts as failed instead of blocking the probe.

```toml
[lifecycle]
shutdown_timeout_secs = 30
```

`run_until(future)` replaces the OS signal with any future, which is useful in tests.
