use common_error::{Error, ErrorKind, Result};
use serde::Deserialize;
use tracing_subscriber::EnvFilter;

pub use tracing;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Pretty,
    #[default]
    Compact,
    Json,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TelemetryConfig {
    pub level: String,
    pub format: LogFormat,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            level: "info".to_owned(),
            format: LogFormat::default(),
        }
    }
}

pub fn init(cfg: &TelemetryConfig) -> Result<()> {
    let filter = match EnvFilter::try_from_default_env() {
        Ok(f) => f,
        Err(_) => EnvFilter::try_new(&cfg.level).map_err(|e| {
            Error::new(
                ErrorKind::Config,
                format!("invalid log level `{}`", cfg.level),
            )
            .with_source(e)
        })?,
    };

    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    let result = match cfg.format {
        LogFormat::Pretty => builder.pretty().try_init(),
        LogFormat::Compact => builder.compact().try_init(),
        LogFormat::Json => builder.json().try_init(),
    };

    result.map_err(|e| Error::internal("failed to install tracing subscriber").with_source(e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults() {
        let cfg = TelemetryConfig::default();
        assert_eq!(cfg.level, "info");
        assert_eq!(cfg.format, LogFormat::Compact);
    }
}
