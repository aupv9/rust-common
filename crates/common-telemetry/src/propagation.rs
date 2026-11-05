use http::HeaderMap;
use http::header::{HeaderName, HeaderValue};
use opentelemetry::global;
use opentelemetry::propagation::{Extractor, Injector};
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

struct HeaderExtractor<'a>(&'a HeaderMap);

impl Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|v| v.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(HeaderName::as_str).collect()
    }
}

struct HeaderInjector<'a>(&'a mut HeaderMap);

impl Injector for HeaderInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(key.as_bytes()),
            HeaderValue::from_str(&value),
        ) {
            self.0.insert(name, value);
        }
    }
}

pub fn set_parent_from_headers(span: &Span, headers: &HeaderMap) {
    let context = global::get_text_map_propagator(|p| p.extract(&HeaderExtractor(headers)));
    let _ = span.set_parent(context);
}

pub fn inject_headers(span: &Span, headers: &mut HeaderMap) {
    let context = span.context();
    global::get_text_map_propagator(|p| p.inject_context(&context, &mut HeaderInjector(headers)));
}
