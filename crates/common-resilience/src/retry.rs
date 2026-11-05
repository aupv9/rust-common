use std::future::Future;
use std::time::Duration;

use common_error::Error;
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub multiplier: f64,
    pub jitter: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_ms: 100,
            max_delay_ms: 5_000,
            multiplier: 2.0,
            jitter: true,
        }
    }
}

impl RetryPolicy {
    #[must_use]
    pub fn none() -> Self {
        Self {
            max_attempts: 1,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn delay_for(&self, retry: u32) -> Duration {
        let max = Duration::from_millis(self.max_delay_ms);
        let exponent = i32::try_from(retry.saturating_sub(1)).unwrap_or(i32::MAX);
        let secs = Duration::from_millis(self.initial_delay_ms).as_secs_f64()
            * self.multiplier.powi(exponent);
        let delay = Duration::try_from_secs_f64(secs).unwrap_or(max).min(max);
        if self.jitter {
            delay.mul_f64(0.5 + fastrand::f64() * 0.5)
        } else {
            delay
        }
    }
}

pub async fn retry<T, F, Fut>(policy: &RetryPolicy, op: F) -> Result<T, Error>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<T, Error>>,
{
    retry_if(policy, |err: &Error| err.kind().is_retryable(), op).await
}

pub async fn retry_if<T, E, F, Fut, P>(
    policy: &RetryPolicy,
    mut should_retry: P,
    mut op: F,
) -> Result<T, E>
where
    E: std::fmt::Display,
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<T, E>>,
    P: FnMut(&E) -> bool,
{
    let max_attempts = policy.max_attempts.max(1);
    let mut attempt = 1;
    loop {
        match op(attempt).await {
            Ok(value) => return Ok(value),
            Err(err) if attempt < max_attempts && should_retry(&err) => {
                let delay = policy.delay_for(attempt);
                tracing::warn!(attempt, max_attempts, delay_ms = u64::try_from(delay.as_millis()).unwrap_or(u64::MAX), error = %err, "retrying");
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
            Err(err) => return Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use common_error::ErrorKind;

    use super::*;

    fn fixed(max_attempts: u32) -> RetryPolicy {
        RetryPolicy {
            max_attempts,
            jitter: false,
            ..RetryPolicy::default()
        }
    }

    #[test]
    fn delay_grows_exponentially_and_is_capped() {
        let policy = RetryPolicy {
            max_delay_ms: 350,
            ..fixed(10)
        };
        let delays: Vec<_> = (1..=4).map(|n| policy.delay_for(n).as_millis()).collect();
        assert_eq!(delays, vec![100, 200, 350, 350]);
    }

    #[test]
    fn huge_retry_count_does_not_overflow() {
        assert_eq!(fixed(10).delay_for(u32::MAX), Duration::from_millis(5_000));
    }

    #[test]
    fn jitter_stays_within_half_to_full_delay() {
        let policy = RetryPolicy {
            jitter: true,
            ..fixed(3)
        };
        for _ in 0..100 {
            let ms = policy.delay_for(2).as_millis();
            assert!((100..=200).contains(&ms), "{ms}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn retries_retryable_errors_until_success() {
        let calls = Cell::new(0);
        let result = retry(&fixed(3), |_| {
            calls.set(calls.get() + 1);
            async {
                if calls.get() < 3 {
                    Err(Error::new(ErrorKind::Unavailable, "down"))
                } else {
                    Ok(42)
                }
            }
        })
        .await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(calls.get(), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn gives_up_after_max_attempts() {
        let calls = Cell::new(0);
        let result: Result<(), _> = retry(&fixed(2), |_| {
            calls.set(calls.get() + 1);
            async { Err(Error::new(ErrorKind::Timeout, "slow")) }
        })
        .await;
        assert_eq!(result.unwrap_err().kind(), ErrorKind::Timeout);
        assert_eq!(calls.get(), 2);
    }

    #[tokio::test]
    async fn non_retryable_error_fails_fast() {
        let calls = Cell::new(0);
        let result: Result<(), _> = retry(&fixed(5), |_| {
            calls.set(calls.get() + 1);
            async { Err(Error::not_found("nope")) }
        })
        .await;
        assert!(result.is_err());
        assert_eq!(calls.get(), 1);
    }
}
