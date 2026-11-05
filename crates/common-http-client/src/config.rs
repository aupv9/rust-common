use common_resilience::{CircuitBreakerConfig, RetryPolicy};
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct HttpClientConfig {
    pub timeout_secs: u64,
    pub connect_timeout_secs: u64,
    pub user_agent: Option<String>,
    pub retry: RetryPolicy,
    pub circuit_breaker: Option<CircuitBreakerConfig>,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            connect_timeout_secs: 5,
            user_agent: None,
            retry: RetryPolicy::default(),
            circuit_breaker: None,
        }
    }
}
