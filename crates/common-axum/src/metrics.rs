use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll, ready};
use std::time::Instant;

use axum::extract::MatchedPath;
use axum::http::{Method, Request, Response, StatusCode, Version};
use opentelemetry::metrics::{Histogram, Meter, UpDownCounter};
use opentelemetry::{InstrumentationScope, KeyValue, global};
use pin_project_lite::pin_project;
use tower::{Layer, Service};

const DURATION_BUCKETS: [f64; 14] = [
    0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 0.75, 1.0, 2.5, 5.0, 7.5, 10.0,
];

struct Instruments {
    duration: Histogram<f64>,
    active: UpDownCounter<i64>,
}

impl Instruments {
    fn new(meter: &Meter) -> Self {
        Self {
            duration: meter
                .f64_histogram("http.server.request.duration")
                .with_unit("s")
                .with_description("Duration of HTTP server requests.")
                .with_boundaries(DURATION_BUCKETS.to_vec())
                .build(),
            active: meter
                .i64_up_down_counter("http.server.active_requests")
                .with_unit("{request}")
                .with_description("Number of active HTTP server requests.")
                .build(),
        }
    }
}

struct Shared(OnceLock<Instruments>);

impl Shared {
    fn instruments(&self) -> &Instruments {
        self.0.get_or_init(|| {
            let scope = InstrumentationScope::builder(env!("CARGO_PKG_NAME"))
                .with_version(env!("CARGO_PKG_VERSION"))
                .build();
            Instruments::new(&global::meter_with_scope(scope))
        })
    }
}

#[derive(Clone)]
pub struct HttpMetricsLayer {
    shared: Arc<Shared>,
}

impl HttpMetricsLayer {
    #[must_use]
    pub fn new(meter: &Meter) -> Self {
        Self {
            shared: Arc::new(Shared(OnceLock::from(Instruments::new(meter)))),
        }
    }

    #[must_use]
    pub fn global() -> Self {
        Self {
            shared: Arc::new(Shared(OnceLock::new())),
        }
    }
}

impl Default for HttpMetricsLayer {
    fn default() -> Self {
        Self::global()
    }
}

impl fmt::Debug for HttpMetricsLayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpMetricsLayer").finish_non_exhaustive()
    }
}

impl<S> Layer<S> for HttpMetricsLayer {
    type Service = HttpMetricsService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        HttpMetricsService {
            inner,
            shared: Arc::clone(&self.shared),
        }
    }
}

#[derive(Clone)]
pub struct HttpMetricsService<S> {
    inner: S,
    shared: Arc<Shared>,
}

impl<S: fmt::Debug> fmt::Debug for HttpMetricsService<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpMetricsService")
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for HttpMetricsService<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>>,
{
    type Response = Response<ResBody>;
    type Error = S::Error;
    type Future = HttpMetricsFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let tracker = InFlight::start(Arc::clone(&self.shared), &req);
        HttpMetricsFuture {
            inner: self.inner.call(req),
            tracker: Some(tracker),
        }
    }
}

pin_project! {
    pub struct HttpMetricsFuture<F> {
        #[pin]
        inner: F,
        tracker: Option<InFlight>,
    }
}

impl<F, ResBody, E> Future for HttpMetricsFuture<F>
where
    F: Future<Output = Result<Response<ResBody>, E>>,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let result = ready!(this.inner.poll(cx));
        if let Some(tracker) = this.tracker.take() {
            tracker.finish(result.as_ref().ok().map(Response::status));
        }
        Poll::Ready(result)
    }
}

struct InFlight {
    shared: Arc<Shared>,
    start: Instant,
    method: &'static str,
    scheme: &'static str,
    version: Option<&'static str>,
    route: Option<String>,
}

impl InFlight {
    fn start<B>(shared: Arc<Shared>, req: &Request<B>) -> Self {
        let tracker = Self {
            shared,
            start: Instant::now(),
            method: method_name(req.method()),
            scheme: if req.uri().scheme_str() == Some("https") {
                "https"
            } else {
                "http"
            },
            version: protocol_version(req.version()),
            route: req
                .extensions()
                .get::<MatchedPath>()
                .map(|path| path.as_str().to_owned()),
        };
        tracker
            .shared
            .instruments()
            .active
            .add(1, &tracker.active_attributes());
        tracker
    }

    fn active_attributes(&self) -> [KeyValue; 2] {
        [
            KeyValue::new("http.request.method", self.method),
            KeyValue::new("url.scheme", self.scheme),
        ]
    }

    fn finish(mut self, status: Option<StatusCode>) {
        let elapsed = self.start.elapsed().as_secs_f64();
        let mut attributes = Vec::with_capacity(6);
        attributes.extend(self.active_attributes());
        if let Some(route) = self.route.take() {
            attributes.push(KeyValue::new("http.route", route));
        }
        if let Some(version) = self.version {
            attributes.push(KeyValue::new("network.protocol.version", version));
        }
        match status {
            Some(status) => {
                attributes.push(KeyValue::new(
                    "http.response.status_code",
                    i64::from(status.as_u16()),
                ));
                if status.is_server_error() {
                    attributes.push(KeyValue::new("error.type", status.as_u16().to_string()));
                }
            }
            None => attributes.push(KeyValue::new("error.type", "_OTHER")),
        }
        self.shared
            .instruments()
            .duration
            .record(elapsed, &attributes);
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        self.shared
            .instruments()
            .active
            .add(-1, &self.active_attributes());
    }
}

fn method_name(method: &Method) -> &'static str {
    match *method {
        Method::GET => "GET",
        Method::POST => "POST",
        Method::PUT => "PUT",
        Method::PATCH => "PATCH",
        Method::DELETE => "DELETE",
        Method::HEAD => "HEAD",
        Method::OPTIONS => "OPTIONS",
        Method::CONNECT => "CONNECT",
        Method::TRACE => "TRACE",
        _ => "_OTHER",
    }
}

fn protocol_version(version: Version) -> Option<&'static str> {
    match version {
        Version::HTTP_09 => Some("0.9"),
        Version::HTTP_10 => Some("1.0"),
        Version::HTTP_11 => Some("1.1"),
        Version::HTTP_2 => Some("2"),
        Version::HTTP_3 => Some("3"),
        _ => None,
    }
}
