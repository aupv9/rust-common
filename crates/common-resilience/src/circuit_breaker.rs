use std::borrow::Cow;
use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use common_error::{Error, ErrorKind, Result};
use serde::Deserialize;
use tokio::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub open_secs: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            open_secs: 30,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Clone, Copy)]
enum State {
    Closed { failures: u32 },
    Open { until: Instant },
    HalfOpen { probe_started: Instant },
}

#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    name: Cow<'static, str>,
    failure_threshold: u32,
    open_for: Duration,
    state: Arc<Mutex<State>>,
}

impl CircuitBreaker {
    pub fn new(name: impl Into<Cow<'static, str>>, config: &CircuitBreakerConfig) -> Self {
        Self {
            name: name.into(),
            failure_threshold: config.failure_threshold.max(1),
            open_for: Duration::from_secs(config.open_secs),
            state: Arc::new(Mutex::new(State::Closed { failures: 0 })),
        }
    }

    #[must_use]
    pub fn state(&self) -> CircuitState {
        match *self.lock() {
            State::Closed { .. } => CircuitState::Closed,
            State::Open { until } if Instant::now() >= until => CircuitState::HalfOpen,
            State::Open { .. } => CircuitState::Open,
            State::HalfOpen { .. } => CircuitState::HalfOpen,
        }
    }

    pub async fn call<T, Fut>(&self, fut: Fut) -> Result<T>
    where
        Fut: Future<Output = Result<T>>,
    {
        self.acquire()?;
        let result = fut.await;
        let failed = matches!(&result, Err(err) if err.kind().is_retryable());
        self.record(failed);
        result
    }

    fn acquire(&self) -> Result<()> {
        let now = Instant::now();
        let mut state = self.lock();
        match *state {
            State::Closed { .. } => Ok(()),
            State::Open { until } if now >= until => {
                *state = State::HalfOpen { probe_started: now };
                tracing::info!(circuit = %self.name, "circuit half-open, probing");
                Ok(())
            }
            State::HalfOpen { probe_started } if now >= probe_started + self.open_for => {
                *state = State::HalfOpen { probe_started: now };
                Ok(())
            }
            State::Open { .. } | State::HalfOpen { .. } => Err(Error::new(
                ErrorKind::Unavailable,
                format!("circuit `{}` is open", self.name),
            )),
        }
    }

    fn record(&self, failed: bool) {
        let mut state = self.lock();
        *state = match (*state, failed) {
            (State::Closed { failures }, true) if failures + 1 >= self.failure_threshold => {
                tracing::warn!(circuit = %self.name, failures = failures + 1, "circuit opened");
                State::Open {
                    until: Instant::now() + self.open_for,
                }
            }
            (State::Closed { failures }, true) => State::Closed {
                failures: failures + 1,
            },
            (State::HalfOpen { .. }, true) => {
                tracing::warn!(circuit = %self.name, "probe failed, circuit re-opened");
                State::Open {
                    until: Instant::now() + self.open_for,
                }
            }
            (State::HalfOpen { .. }, false) => {
                tracing::info!(circuit = %self.name, "circuit closed");
                State::Closed { failures: 0 }
            }
            (State::Closed { .. }, false) => State::Closed { failures: 0 },
            (open @ State::Open { .. }, _) => open,
        };
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn breaker() -> CircuitBreaker {
        CircuitBreaker::new(
            "upstream",
            &CircuitBreakerConfig {
                failure_threshold: 2,
                open_secs: 10,
            },
        )
    }

    async fn fail(cb: &CircuitBreaker) -> Result<()> {
        cb.call(async { Err(Error::new(ErrorKind::Unavailable, "down")) })
            .await
    }

    async fn succeed(cb: &CircuitBreaker) -> Result<()> {
        cb.call(async { Ok(()) }).await
    }

    #[tokio::test(start_paused = true)]
    async fn opens_after_threshold_and_rejects_calls() {
        let cb = breaker();
        fail(&cb).await.unwrap_err();
        assert_eq!(cb.state(), CircuitState::Closed);
        fail(&cb).await.unwrap_err();
        assert_eq!(cb.state(), CircuitState::Open);

        let err = succeed(&cb).await.unwrap_err();
        assert_eq!(err.message(), "circuit `upstream` is open");
    }

    #[tokio::test(start_paused = true)]
    async fn half_open_probe_success_closes() {
        let cb = breaker();
        fail(&cb).await.unwrap_err();
        fail(&cb).await.unwrap_err();
        tokio::time::advance(Duration::from_secs(11)).await;
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        succeed(&cb).await.unwrap();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[tokio::test(start_paused = true)]
    async fn half_open_probe_failure_reopens() {
        let cb = breaker();
        fail(&cb).await.unwrap_err();
        fail(&cb).await.unwrap_err();
        tokio::time::advance(Duration::from_secs(11)).await;

        fail(&cb).await.unwrap_err();
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[tokio::test]
    async fn business_errors_do_not_trip_the_breaker() {
        let cb = breaker();
        for _ in 0..5 {
            cb.call(async { Err::<(), _>(Error::not_found("x")) })
                .await
                .unwrap_err();
        }
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn success_resets_failure_count() {
        let cb = breaker();
        fail(&cb).await.unwrap_err();
        succeed(&cb).await.unwrap();
        fail(&cb).await.unwrap_err();
        assert_eq!(cb.state(), CircuitState::Closed);
    }
}
