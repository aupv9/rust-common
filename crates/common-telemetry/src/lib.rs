//! Tracing/logging initialisation.
//!
//! `RUST_LOG`, when set, takes precedence over [`TelemetryConfig::level`].

use common_error::{Error, ErrorKind, Result};
use serde::Deserialize;
use tracing_subscriber::EnvFilter;

pub use tracing;

/// Log output format.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Multi-line, human friendly (local development).
    Pretty,
    /// Single-line, human friendly.
    #[default]
    Compact,
    /// Structured JSON (production / log shipping).
    Json,
}

/// Telemetry settings; can be embedded in an app config.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TelemetryConfig {
    /// Default filter directive, e.g. `info` or `my_app=debug,info`.
    pub level: String,
    /// Output format.
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

/// Installs the global tracing subscriber.
///
/// # Errors
///
/// Returns an error if the filter directive is invalid or a global
/// subscriber has already been installed.
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
