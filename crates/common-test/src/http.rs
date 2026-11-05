use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use bytes::Bytes;
use http_body_util::BodyExt;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tower::ServiceExt;

#[derive(Clone)]
pub struct TestApp {
    router: Router,
}

impl TestApp {
    #[must_use]
    pub fn new(router: Router) -> Self {
        Self { router }
    }

    pub async fn get(&self, uri: &str) -> TestResponse {
        self.send(Method::GET, uri, Body::empty(), None).await
    }

    pub async fn delete(&self, uri: &str) -> TestResponse {
        self.send(Method::DELETE, uri, Body::empty(), None).await
    }

    pub async fn post_json(&self, uri: &str, body: &impl Serialize) -> TestResponse {
        self.send_json(Method::POST, uri, body).await
    }

    pub async fn put_json(&self, uri: &str, body: &impl Serialize) -> TestResponse {
        self.send_json(Method::PUT, uri, body).await
    }

    pub async fn patch_json(&self, uri: &str, body: &impl Serialize) -> TestResponse {
        self.send_json(Method::PATCH, uri, body).await
    }

    pub async fn request(&self, request: Request<Body>) -> TestResponse {
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("router is infallible");
        let status = response.status();
        let headers = response.headers().clone();
        let body = response
            .into_body()
            .collect()
            .await
            .expect("failed to read response body")
            .to_bytes();
        TestResponse {
            status,
            headers,
            body,
        }
    }

    async fn send_json(&self, method: Method, uri: &str, body: &impl Serialize) -> TestResponse {
        let bytes = serde_json::to_vec(body).expect("failed to serialize request body");
        self.send(method, uri, Body::from(bytes), Some("application/json"))
            .await
    }

    async fn send(
        &self,
        method: Method,
        uri: &str,
        body: Body,
        content_type: Option<&str>,
    ) -> TestResponse {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(content_type) = content_type {
            builder = builder.header(header::CONTENT_TYPE, content_type);
        }
        self.request(builder.body(body).expect("invalid request"))
            .await
    }
}

#[derive(Debug, Clone)]
pub struct TestResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl TestResponse {
    #[must_use]
    pub fn json<T: DeserializeOwned>(&self) -> T {
        serde_json::from_slice(&self.body).unwrap_or_else(|e| {
            panic!(
                "response body is not valid JSON ({e}): {}",
                String::from_utf8_lossy(&self.body)
            )
        })
    }

    #[must_use]
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    #[track_caller]
    pub fn assert_status(&self, expected: StatusCode) -> &Self {
        assert_eq!(
            self.status,
            expected,
            "unexpected status, body: {}",
            self.text()
        );
        self
    }

    #[track_caller]
    pub fn assert_problem(&self, status: StatusCode, code: &str) -> &Self {
        self.assert_status(status);
        let content_type = self
            .headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok());
        assert_eq!(
            content_type,
            Some("application/problem+json"),
            "not a problem+json response"
        );
        let body: serde_json::Value = self.json();
        assert_eq!(body["code"], code, "unexpected problem code, body: {body}");
        self
    }
}

#[cfg(test)]
mod tests {
    use axum::routing::{get, post};
    use serde_json::{Value, json};

    use super::*;

    fn app() -> TestApp {
        TestApp::new(
            Router::new()
                .route("/ping", get(|| async { "pong" }))
                .route(
                    "/echo",
                    post(|axum::Json(v): axum::Json<Value>| async move { axum::Json(v) }),
                ),
        )
    }

    #[tokio::test]
    async fn get_and_post_json() {
        let app = app();
        assert_eq!(
            app.get("/ping").await.assert_status(StatusCode::OK).text(),
            "pong"
        );
        let res = app.post_json("/echo", &json!({ "a": 1 })).await;
        assert_eq!(res.json::<Value>(), json!({ "a": 1 }));
    }
}
