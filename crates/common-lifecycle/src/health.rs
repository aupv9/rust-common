use std::borrow::Cow;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, PoisonError, RwLock};
use std::time::{Duration, Instant};

use common_error::Result;
use futures_util::future::join_all;
use serde::Serialize;
use tokio_util::sync::CancellationToken;

const DEFAULT_CHECK_TIMEOUT: Duration = Duration::from_secs(5);

type CheckFuture = Pin<Box<dyn Future<Output = Result<()>> + Send>>;
type CheckFn = Arc<dyn Fn() -> CheckFuture + Send + Sync>;
type Checks = Arc<RwLock<Vec<(Cow<'static, str>, CheckFn)>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Up,
    Down,
    Draining,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    pub name: Cow<'static, str>,
    pub status: HealthStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct HealthReport {
    pub status: HealthStatus,
    pub checks: Vec<CheckResult>,
}

impl HealthReport {
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.status == HealthStatus::Up
    }
}

#[derive(Clone)]
pub struct HealthRegistry {
    checks: Checks,
    draining: CancellationToken,
    timeout: Duration,
}

impl Default for HealthRegistry {
    fn default() -> Self {
        Self::with_draining_token(CancellationToken::new())
    }
}

impl HealthRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_draining_token(draining: CancellationToken) -> Self {
        Self {
            checks: Arc::default(),
            draining,
            timeout: DEFAULT_CHECK_TIMEOUT,
        }
    }

    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn register<F, Fut>(&self, name: impl Into<Cow<'static, str>>, check: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        let check: CheckFn = Arc::new(move || Box::pin(check()));
        self.checks
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .push((name.into(), check));
    }

    pub async fn check(&self) -> HealthReport {
        let checks = self
            .checks
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let timeout = self.timeout;

        let results = join_all(checks.into_iter().map(|(name, check)| async move {
            let started = Instant::now();
            let outcome = tokio::time::timeout(timeout, check()).await;
            let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            let error = match outcome {
                Ok(Ok(())) => None,
                Ok(Err(err)) => Some(err.to_string()),
                Err(_) => Some(format!("timed out after {timeout:?}")),
            };
            let status = if error.is_some() {
                HealthStatus::Down
            } else {
                HealthStatus::Up
            };
            CheckResult {
                name,
                status,
                error,
                duration_ms,
            }
        }))
        .await;

        let status = if self.draining.is_cancelled() {
            HealthStatus::Draining
        } else if results.iter().all(|r| r.status == HealthStatus::Up) {
            HealthStatus::Up
        } else {
            HealthStatus::Down
        };

        HealthReport {
            status,
            checks: results,
        }
    }
}

#[cfg(test)]
mod tests {
    use common_error::Error;

    use super::*;

    #[tokio::test]
    async fn empty_registry_is_up() {
        assert!(HealthRegistry::new().check().await.is_ready());
    }

    #[tokio::test]
    async fn failing_check_marks_down() {
        let registry = HealthRegistry::new();
        registry.register("db", || async { Ok(()) });
        registry.register("cache", || async { Err(Error::internal("refused")) });

        let report = registry.check().await;
        assert_eq!(report.status, HealthStatus::Down);
        let cache = report.checks.iter().find(|c| c.name == "cache").unwrap();
        assert_eq!(cache.error.as_deref(), Some("INTERNAL: refused"));
    }

    #[tokio::test(start_paused = true)]
    async fn slow_check_times_out() {
        let registry = HealthRegistry::new().with_timeout(Duration::from_millis(10));
        registry.register("slow", std::future::pending);

        let report = registry.check().await;
        assert_eq!(report.status, HealthStatus::Down);
    }

    #[tokio::test]
    async fn draining_is_not_ready() {
        let token = CancellationToken::new();
        let registry = HealthRegistry::with_draining_token(token.clone());
        token.cancel();

        let report = registry.check().await;
        assert_eq!(report.status, HealthStatus::Draining);
        assert_eq!(serde_json::to_value(&report).unwrap()["status"], "draining");
    }
}
