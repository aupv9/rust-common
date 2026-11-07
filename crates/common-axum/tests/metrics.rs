#![cfg(feature = "otel")]

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use common_axum::{HttpMetricsLayer, ServerConfig, app};
use common_lifecycle::HealthRegistry;
use opentelemetry::global;
use opentelemetry::metrics::MeterProvider as _;
use opentelemetry_sdk::metrics::data::{AggregatedMetrics, MetricData};
use opentelemetry_sdk::metrics::{InMemoryMetricExporter, SdkMeterProvider};
use tokio::sync::Notify;
use tower::ServiceExt;

type Attrs = BTreeMap<String, String>;

struct Recorder {
    provider: SdkMeterProvider,
    exporter: InMemoryMetricExporter,
}

#[derive(Debug, Default)]
struct Snapshot {
    durations: Vec<(Attrs, u64)>,
    active: Vec<(Attrs, i64)>,
}

impl Snapshot {
    fn duration_count(&self, expected: &[(&str, &str)]) -> u64 {
        let expected = attrs(expected);
        self.durations
            .iter()
            .filter(|(a, _)| *a == expected)
            .map(|(_, count)| count)
            .sum()
    }

    fn has_route(&self, route: &str) -> bool {
        self.durations
            .iter()
            .any(|(a, _)| a.get("http.route").map(String::as_str) == Some(route))
    }

    fn active_total(&self) -> i64 {
        self.active.iter().map(|(_, v)| v).sum()
    }
}

impl Recorder {
    fn new() -> Self {
        let exporter = InMemoryMetricExporter::default();
        let provider = SdkMeterProvider::builder()
            .with_periodic_exporter(exporter.clone())
            .build();
        Self { provider, exporter }
    }

    fn layer(&self) -> HttpMetricsLayer {
        HttpMetricsLayer::new(&self.provider.meter("test"))
    }

    fn snapshot(&self) -> Snapshot {
        self.exporter.reset();
        self.provider.force_flush().unwrap();
        let mut snapshot = Snapshot::default();
        for resource in self.exporter.get_finished_metrics().unwrap() {
            for scope in resource.scope_metrics() {
                for metric in scope.metrics() {
                    match (metric.name(), metric.data()) {
                        (
                            "http.server.request.duration",
                            AggregatedMetrics::F64(MetricData::Histogram(h)),
                        ) => {
                            assert_eq!(metric.unit(), "s");
                            for p in h.data_points() {
                                assert_eq!(p.bounds().count(), 14);
                                snapshot
                                    .durations
                                    .push((collect(p.attributes()), p.count()));
                            }
                        }
                        (
                            "http.server.active_requests",
                            AggregatedMetrics::I64(MetricData::Sum(s)),
                        ) => {
                            assert!(!s.is_monotonic());
                            for p in s.data_points() {
                                snapshot.active.push((collect(p.attributes()), p.value()));
                            }
                        }
                        (name, _) => panic!("unexpected metric {name}"),
                    }
                }
            }
        }
        snapshot
    }
}

fn collect<'a>(kvs: impl Iterator<Item = &'a opentelemetry::KeyValue>) -> Attrs {
    kvs.map(|kv| (kv.key.to_string(), kv.value.to_string()))
        .collect()
}

fn attrs(pairs: &[(&str, &str)]) -> Attrs {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

fn get_req(uri: &str) -> Request<Body> {
    Request::get(uri).body(Body::empty()).unwrap()
}

async fn call(app: &Router, req: Request<Body>) -> StatusCode {
    app.clone().oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn records_duration_per_route_and_status() {
    let recorder = Recorder::new();
    let app = Router::new()
        .route("/orders/{id}", get(|| async { "ok" }))
        .route("/boom", get(|| async { StatusCode::INTERNAL_SERVER_ERROR }))
        .layer(recorder.layer());

    assert_eq!(call(&app, get_req("/orders/1")).await, StatusCode::OK);
    assert_eq!(call(&app, get_req("/orders/2")).await, StatusCode::OK);
    assert_eq!(
        call(&app, get_req("/boom")).await,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        call(&app, get_req("/does-not-exist")).await,
        StatusCode::NOT_FOUND
    );
    let custom = Request::builder()
        .method("PURGE")
        .uri("/orders/3")
        .body(Body::empty())
        .unwrap();
    call(&app, custom).await;

    let snapshot = recorder.snapshot();
    let base = [
        ("http.request.method", "GET"),
        ("url.scheme", "http"),
        ("network.protocol.version", "1.1"),
    ];
    let with = |extra: &[(&'static str, &'static str)]| {
        let mut all = base.to_vec();
        all.extend_from_slice(extra);
        all
    };

    assert_eq!(
        snapshot.duration_count(&with(&[
            ("http.route", "/orders/{id}"),
            ("http.response.status_code", "200"),
        ])),
        2
    );
    assert_eq!(
        snapshot.duration_count(&with(&[
            ("http.route", "/boom"),
            ("http.response.status_code", "500"),
            ("error.type", "500"),
        ])),
        1
    );
    assert_eq!(
        snapshot.duration_count(&with(&[("http.response.status_code", "404")])),
        1,
        "unmatched paths carry no http.route"
    );
    assert_eq!(
        snapshot.duration_count(&[
            ("http.request.method", "_OTHER"),
            ("url.scheme", "http"),
            ("network.protocol.version", "1.1"),
            ("http.route", "/orders/{id}"),
            ("http.response.status_code", "405"),
        ]),
        1
    );
    assert_eq!(snapshot.active_total(), 0);
}

#[derive(Clone, Default)]
struct Gate {
    entered: Arc<Notify>,
    release: Arc<Notify>,
}

async fn gated(State(gate): State<Gate>) -> &'static str {
    gate.entered.notify_one();
    gate.release.notified().await;
    "done"
}

fn gated_app(recorder: &Recorder, gate: Gate) -> Router {
    Router::new()
        .route("/wait", get(gated))
        .with_state(gate)
        .layer(recorder.layer())
}

#[tokio::test]
async fn tracks_active_requests() {
    let recorder = Recorder::new();
    let gate = Gate::default();
    let app = gated_app(&recorder, gate.clone());

    let request = tokio::spawn(async move { call(&app, get_req("/wait")).await });
    gate.entered.notified().await;

    let during = recorder.snapshot();
    assert_eq!(
        during.active,
        vec![(
            attrs(&[("http.request.method", "GET"), ("url.scheme", "http")]),
            1
        )]
    );
    assert!(!during.has_route("/wait"));

    gate.release.notify_one();
    assert_eq!(request.await.unwrap(), StatusCode::OK);

    let after = recorder.snapshot();
    assert_eq!(after.active_total(), 0);
    assert!(after.has_route("/wait"));
}

#[tokio::test]
async fn cancelled_request_is_no_longer_active() {
    let recorder = Recorder::new();
    let gate = Gate::default();
    let app = gated_app(&recorder, gate.clone());

    let request = tokio::spawn(async move { call(&app, get_req("/wait")).await });
    gate.entered.notified().await;
    assert_eq!(recorder.snapshot().active_total(), 1);

    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());

    let after = recorder.snapshot();
    assert_eq!(after.active_total(), 0);
    assert!(!after.has_route("/wait"));
}

async fn panics() -> &'static str {
    panic!("kaboom")
}

#[tokio::test(start_paused = true)]
async fn default_stack_records_through_global_provider() {
    let recorder = Recorder::new();
    global::set_meter_provider(recorder.provider.clone());

    let routes = Router::new()
        .route(
            "/slow",
            get(|| async {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                "late"
            }),
        )
        .route("/panic", get(panics));
    let cfg = ServerConfig {
        request_timeout_secs: 1,
        ..ServerConfig::default()
    };
    let app = app(routes, &cfg, HealthRegistry::new()).unwrap();

    assert_eq!(
        call(&app, get_req("/slow")).await,
        StatusCode::GATEWAY_TIMEOUT
    );
    assert_eq!(
        call(&app, get_req("/panic")).await,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(call(&app, get_req("/health/live")).await, StatusCode::OK);

    let snapshot = recorder.snapshot();
    let expect = |route, status| {
        vec![
            ("http.request.method", "GET"),
            ("url.scheme", "http"),
            ("network.protocol.version", "1.1"),
            ("http.route", route),
            ("http.response.status_code", status),
            ("error.type", status),
        ]
    };
    assert_eq!(snapshot.duration_count(&expect("/slow", "504")), 1);
    assert_eq!(snapshot.duration_count(&expect("/panic", "500")), 1);
    assert!(
        !snapshot.has_route("/health/live"),
        "health probes are not measured"
    );
}
