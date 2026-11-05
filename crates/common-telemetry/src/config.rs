use std::borrow::Cow;

use serde::Deserialize;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Pretty,
    #[default]
    Compact,
    Json,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TelemetryConfig {
    pub level: LogLevel,
    pub format: LogFormat,
    pub otlp: Option<OtlpConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(transparent)]
pub struct LogLevel(pub String);

impl Default for LogLevel {
    fn default() -> Self {
        Self("info".to_owned())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct OtlpConfig {
    pub endpoint: String,
    pub sample_ratio: f64,
    pub metrics: bool,
}

impl Default for OtlpConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:4317".to_owned(),
            sample_ratio: 1.0,
            metrics: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServiceInfo {
    pub name: Cow<'static, str>,
    pub version: Cow<'static, str>,
    pub environment: Option<String>,
}

impl ServiceInfo {
    pub fn new(name: impl Into<Cow<'static, str>>, version: impl Into<Cow<'static, str>>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            environment: None,
        }
    }

    #[must_use]
    pub fn with_environment(mut self, environment: impl Into<String>) -> Self {
        self.environment = Some(environment.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let cfg = TelemetryConfig::default();
        assert_eq!(cfg.level.0, "info");
        assert_eq!(cfg.format, LogFormat::Compact);
        assert!(cfg.otlp.is_none());
    }

    #[test]
    fn otlp_section_enables_export_with_defaults() {
        let cfg: TelemetryConfig =
            serde_json::from_str(r#"{"level": "debug", "format": "json", "otlp": {}}"#).unwrap();
        let otlp = cfg.otlp.unwrap();
        assert_eq!(otlp.endpoint, "http://localhost:4317");
        assert!(otlp.metrics);
    }
}
