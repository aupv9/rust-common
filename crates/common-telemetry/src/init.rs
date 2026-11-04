use common_error::{Error, ErrorKind, Result};
use tracing::Subscriber;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use crate::config::{LogFormat, ServiceInfo, TelemetryConfig};

#[must_use = "dropping the guard flushes and shuts down telemetry exporters"]
#[derive(Default)]
pub struct TelemetryGuard {
    #[cfg(feature = "otel")]
    otel: Option<crate::otel::OtelProviders>,
}

impl std::fmt::Debug for TelemetryGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelemetryGuard").finish_non_exhaustive()
    }
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        #[cfg(feature = "otel")]
        if let Some(providers) = self.otel.take() {
            providers.shutdown();
        }
    }
}

pub fn init(service: &ServiceInfo, cfg: &TelemetryConfig) -> Result<TelemetryGuard> {
    let filter = build_filter(&cfg.level.0)?;
    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer(cfg.format));

    #[cfg(feature = "otel")]
    {
        let providers = cfg
            .otlp
            .as_ref()
            .map(|otlp| crate::otel::build(service, otlp))
            .transpose()?;
        let otel_layer = providers.as_ref().map(crate::otel::OtelProviders::layer);
        registry
            .with(otel_layer)
            .try_init()
            .map_err(install_error)?;
        tracing::info!(service = %service.name, version = %service.version, otlp = providers.is_some(), "telemetry initialised");
        Ok(TelemetryGuard { otel: providers })
    }

    #[cfg(not(feature = "otel"))]
    {
        registry.try_init().map_err(install_error)?;
        if cfg.otlp.is_some() {
            tracing::warn!("`telemetry.otlp` is configured but the `otel` feature is disabled");
        }
        tracing::info!(service = %service.name, version = %service.version, "telemetry initialised");
        Ok(TelemetryGuard::default())
    }
}

fn build_filter(level: &str) -> Result<EnvFilter> {
    match EnvFilter::try_from_default_env() {
        Ok(filter) => Ok(filter),
        Err(_) => EnvFilter::try_new(level).map_err(|e| {
            Error::new(ErrorKind::Config, format!("invalid log level `{level}`")).with_source(e)
        }),
    }
}

fn fmt_layer<S>(format: LogFormat) -> Box<dyn Layer<S> + Send + Sync>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    let layer = tracing_subscriber::fmt::layer();
    match format {
        LogFormat::Pretty => layer.pretty().boxed(),
        LogFormat::Compact => layer.compact().boxed(),
        LogFormat::Json => layer
            .json()
            .flatten_event(true)
            .with_current_span(true)
            .boxed(),
    }
}

fn install_error(err: impl Into<common_error::BoxError>) -> Error {
    Error::internal("failed to install tracing subscriber").with_source(err)
}
