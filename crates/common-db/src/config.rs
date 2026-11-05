use common_types::Secret;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct DbConfig {
    pub url: Secret<String>,
    #[serde(flatten)]
    pub pool: PoolConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct PoolConfig {
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout_secs: u64,
    pub idle_timeout_secs: Option<u64>,
    pub max_lifetime_secs: Option<u64>,
    pub statement_timeout_ms: Option<u64>,
    pub application_name: Option<String>,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            min_connections: 0,
            acquire_timeout_secs: 5,
            idle_timeout_secs: Some(600),
            max_lifetime_secs: Some(1800),
            statement_timeout_ms: None,
            application_name: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_fields_are_flattened_with_defaults() {
        let cfg: DbConfig =
            serde_json::from_str(r#"{"url": "postgres://u:p@h/db", "max_connections": 20}"#)
                .unwrap();
        assert_eq!(cfg.pool.max_connections, 20);
        assert_eq!(cfg.pool.acquire_timeout_secs, 5);
        assert_eq!(format!("{:?}", cfg.url), "Secret(***)");
    }
}
