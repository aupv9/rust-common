use common_error::{Error, ErrorKind, Result};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{KeyValue, global};
use opentelemetry_otlp::{MetricExporter, SpanExporter, WithExportConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider, Tracer};
use tracing::Subscriber;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::registry::LookupSpan;

use crate::config::{OtlpConfig, ServiceInfo};

pub(crate) struct OtelProviders {
    tracer: Tracer,
    tracer_provider: SdkTracerProvider,
    meter_provider: Option<SdkMeterProvider>,
}

impl OtelProviders {
    pub(crate) fn layer<S>(&self) -> OpenTelemetryLayer<S, Tracer>
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        tracing_opentelemetry::layer().with_tracer(self.tracer.clone())
    }

    pub(crate) fn shutdown(self) {
        if let Err(err) = self.tracer_provider.shutdown() {
            eprintln!("failed to shut down tracer provider: {err}");
        }
        if let Some(meter_provider) = self.meter_provider
            && let Err(err) = meter_provider.shutdown()
        {
            eprintln!("failed to shut down meter provider: {err}");
        }
    }
}

pub(crate) fn build(service: &ServiceInfo, cfg: &OtlpConfig) -> Result<OtelProviders> {
    let resource = resource(service);

    let span_exporter = SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&cfg.endpoint)
        .build()
        .map_err(|e| exporter_error("span", e))?;

    let sampler = Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(cfg.sample_ratio)));
    let tracer_provider = SdkTracerProvider::builder()
        .with_batch_exporter(span_exporter)
        .with_sampler(sampler)
        .with_resource(resource.clone())
        .build();
    let tracer = tracer_provider.tracer(service.name.clone());

    global::set_text_map_propagator(TraceContextPropagator::new());
    global::set_tracer_provider(tracer_provider.clone());

    let meter_provider = if cfg.metrics {
        let metric_exporter = MetricExporter::builder()
            .with_tonic()
            .with_endpoint(&cfg.endpoint)
            .build()
            .map_err(|e| exporter_error("metric", e))?;
        let provider = SdkMeterProvider::builder()
            .with_periodic_exporter(metric_exporter)
            .with_resource(resource)
            .build();
        global::set_meter_provider(provider.clone());
        Some(provider)
    } else {
        None
    };

    Ok(OtelProviders {
        tracer,
        tracer_provider,
        meter_provider,
    })
}

fn resource(service: &ServiceInfo) -> Resource {
    let mut attributes = vec![KeyValue::new(
        "service.version",
        service.version.to_string(),
    )];
    if let Some(env) = &service.environment {
        attributes.push(KeyValue::new("deployment.environment.name", env.clone()));
    }
    Resource::builder()
        .with_service_name(service.name.to_string())
        .with_attributes(attributes)
        .build()
}

fn exporter_error(signal: &str, err: impl Into<common_error::BoxError>) -> Error {
    Error::new(
        ErrorKind::Config,
        format!("failed to build OTLP {signal} exporter"),
    )
    .with_source(err)
}
