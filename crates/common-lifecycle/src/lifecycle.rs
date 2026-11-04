use std::borrow::Cow;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use common_error::{Error, ErrorKind, Result};
use futures_util::FutureExt;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::health::HealthRegistry;
use crate::signal::shutdown_signal;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LifecycleConfig {
    pub shutdown_timeout_secs: u64,
}

impl Default for LifecycleConfig {
    fn default() -> Self {
        Self {
            shutdown_timeout_secs: 30,
        }
    }
}

#[derive(Clone)]
pub struct Lifecycle {
    inner: Arc<Inner>,
}

struct Inner {
    token: CancellationToken,
    tracker: TaskTracker,
    health: HealthRegistry,
    failure: Mutex<Option<Error>>,
    shutdown_timeout: Duration,
}

impl Lifecycle {
    #[must_use]
    pub fn new(config: &LifecycleConfig) -> Self {
        let token = CancellationToken::new();
        Self {
            inner: Arc::new(Inner {
                health: HealthRegistry::with_draining_token(token.clone()),
                token,
                tracker: TaskTracker::new(),
                failure: Mutex::new(None),
                shutdown_timeout: Duration::from_secs(config.shutdown_timeout_secs),
            }),
        }
    }

    #[must_use]
    pub fn token(&self) -> CancellationToken {
        self.inner.token.clone()
    }

    #[must_use]
    pub fn health(&self) -> &HealthRegistry {
        &self.inner.health
    }

    #[must_use]
    pub fn is_shutting_down(&self) -> bool {
        self.inner.token.is_cancelled()
    }

    pub fn shutdown(&self) {
        self.inner.token.cancel();
    }

    pub fn spawn<F, Fut>(&self, name: impl Into<Cow<'static, str>>, task: F)
    where
        F: FnOnce(CancellationToken) -> Fut,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        let name = name.into();
        let fut = AssertUnwindSafe(task(self.inner.token.clone())).catch_unwind();
        let inner = Arc::clone(&self.inner);

        self.inner.tracker.spawn(async move {
            let outcome = match fut.await {
                Ok(result) => result,
                Err(_) => Err(Error::internal(format!("task `{name}` panicked"))),
            };
            match outcome {
                Ok(()) => tracing::debug!(task = %name, "task finished"),
                Err(err) => {
                    tracing::error!(task = %name, error = %err, "task failed, shutting down");
                    inner
                        .failure
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .get_or_insert(err);
                    inner.token.cancel();
                }
            }
        });
    }

    pub async fn run(self) -> Result<()> {
        self.run_until(shutdown_signal()).await
    }

    pub async fn run_until(self, signal: impl Future<Output = ()>) -> Result<()> {
        let inner = &self.inner;
        tokio::select! {
            () = signal => tracing::info!("shutdown requested"),
            () = inner.token.cancelled() => {}
        }
        inner.token.cancel();
        inner.tracker.close();

        tracing::info!(timeout = ?inner.shutdown_timeout, tasks = inner.tracker.len(), "draining tasks");
        if tokio::time::timeout(inner.shutdown_timeout, inner.tracker.wait())
            .await
            .is_err()
        {
            return Err(Error::new(
                ErrorKind::Timeout,
                "graceful shutdown timed out",
            ));
        }
        tracing::info!("shutdown complete");

        match inner
            .failure
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    fn lifecycle(timeout_secs: u64) -> Lifecycle {
        Lifecycle::new(&LifecycleConfig {
            shutdown_timeout_secs: timeout_secs,
        })
    }

    #[tokio::test]
    async fn tasks_are_cancelled_and_awaited_on_signal() {
        let lc = lifecycle(5);
        let stopped = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stopped);
        lc.spawn("worker", |token| async move {
            token.cancelled().await;
            flag.store(true, Ordering::SeqCst);
            Ok(())
        });

        lc.run_until(async {}).await.unwrap();
        assert!(stopped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn failing_task_triggers_shutdown_and_is_reported() {
        let lc = lifecycle(5);
        lc.spawn("bad", |_| async { Err(Error::conflict("boom")) });
        lc.spawn("good", |token| async move {
            token.cancelled().await;
            Ok(())
        });

        let err = lc.run_until(std::future::pending()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Conflict);
    }

    #[tokio::test]
    async fn panicking_task_is_reported_as_internal() {
        let lc = lifecycle(5);
        lc.spawn("panics", |_| async { panic!("oops") });

        let err = lc.run_until(std::future::pending()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Internal);
    }

    #[tokio::test(start_paused = true)]
    async fn stuck_task_causes_timeout() {
        let lc = lifecycle(1);
        lc.spawn("stuck", |_| std::future::pending());

        let err = lc.run_until(async {}).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Timeout);
    }
}
