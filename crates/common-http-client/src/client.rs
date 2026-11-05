use std::borrow::Cow;
use std::time::Duration;

use common_error::{Error, ErrorKind, Result};
use common_resilience::{CircuitBreaker, RetryPolicy, retry};
use reqwest::{IntoUrl, Method, Request, RequestBuilder, Response, Url};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use tracing::Instrument;

use crate::config::HttpClientConfig;

const MAX_ERROR_BODY: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub struct HttpClient {
    name: Cow<'static, str>,
    inner: reqwest::Client,
    retry: RetryPolicy,
    breaker: Option<CircuitBreaker>,
}

impl HttpClient {
    pub fn new(name: impl Into<Cow<'static, str>>, config: &HttpClientConfig) -> Result<Self> {
        let name = name.into();
        let user_agent = config
            .user_agent
            .clone()
            .unwrap_or_else(|| name.to_string());
        let inner = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .connect_timeout(Duration::from_secs(config.connect_timeout_secs))
            .user_agent(user_agent)
            .build()
            .map_err(|e| {
                Error::new(ErrorKind::Config, "failed to build http client").with_source(e)
            })?;
        let breaker = config
            .circuit_breaker
            .as_ref()
            .map(|cfg| CircuitBreaker::new(name.clone(), cfg));
        Ok(Self {
            name,
            inner,
            retry: config.retry.clone(),
            breaker,
        })
    }

    #[must_use]
    pub fn inner(&self) -> &reqwest::Client {
        &self.inner
    }

    #[must_use]
    pub fn circuit_breaker(&self) -> Option<&CircuitBreaker> {
        self.breaker.as_ref()
    }

    pub fn request(&self, method: Method, url: impl IntoUrl) -> RequestBuilder {
        self.inner.request(method, url)
    }

    pub fn get(&self, url: impl IntoUrl) -> RequestBuilder {
        self.inner.get(url)
    }

    pub fn post(&self, url: impl IntoUrl) -> RequestBuilder {
        self.inner.post(url)
    }

    pub fn put(&self, url: impl IntoUrl) -> RequestBuilder {
        self.inner.put(url)
    }

    pub fn patch(&self, url: impl IntoUrl) -> RequestBuilder {
        self.inner.patch(url)
    }

    pub fn delete(&self, url: impl IntoUrl) -> RequestBuilder {
        self.inner.delete(url)
    }

    pub async fn send_json<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T> {
        let response = self.send(request).await?;
        Ok(response.json().await?)
    }

    pub async fn send(&self, request: RequestBuilder) -> Result<Response> {
        let request = request.build()?;
        let method = request.method().clone();
        let url = redact(request.url());
        let span = tracing::info_span!(
            "http.client",
            otel.name = %method,
            otel.kind = "client",
            peer.service = %self.name,
            http.request.method = %method,
            url.full = %url,
            http.response.status_code = tracing::field::Empty,
        );

        let policy = if method.is_idempotent() && request.try_clone().is_some() {
            self.retry.clone()
        } else {
            RetryPolicy::none()
        };

        async move {
            let mut original = Some(request);
            let result = retry(&policy, |_| {
                let next = match original.as_ref().and_then(Request::try_clone) {
                    Some(copy) if policy.max_attempts > 1 => Some(copy),
                    _ => original.take(),
                };
                async move {
                    let request =
                        next.ok_or_else(|| Error::internal("request cannot be retried"))?;
                    match &self.breaker {
                        Some(breaker) => breaker.call(self.execute_once(request)).await,
                        None => self.execute_once(request).await,
                    }
                }
            })
            .await;
            if let Err(err) = &result {
                tracing::warn!(error = %err, "upstream request failed");
            }
            result
        }
        .instrument(span)
        .await
    }

    async fn execute_once(&self, mut request: Request) -> Result<Response> {
        #[cfg(feature = "otel")]
        common_telemetry::propagation::inject_headers(
            &tracing::Span::current(),
            request.headers_mut(),
        );
        #[cfg(not(feature = "otel"))]
        let _ = &mut request;

        let response = self.inner.execute(request).await?;
        let status = response.status();
        tracing::Span::current().record("http.response.status_code", status.as_u16());

        if status.is_success() || status.is_redirection() {
            Ok(response)
        } else {
            Err(error_from_response(response).await)
        }
    }
}

#[derive(Deserialize)]
struct Problem {
    detail: Option<String>,
    code: Option<String>,
}

async fn error_from_response(response: Response) -> Error {
    let status = response.status();
    let kind = ErrorKind::from_http_status(status.as_u16());
    let is_problem = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("application/problem+json"));

    let fallback = || Error::new(kind, format!("upstream returned {status}"));
    if !is_problem
        || response
            .content_length()
            .is_some_and(|len| len > MAX_ERROR_BODY as u64)
    {
        return fallback();
    }

    match response
        .bytes()
        .await
        .ok()
        .and_then(|b| serde_json::from_slice::<Problem>(&b).ok())
    {
        Some(problem) => {
            let message = problem
                .detail
                .unwrap_or_else(|| format!("upstream returned {status}"));
            let error = Error::new(kind, message);
            match problem.code {
                Some(code) => error.with_code(code),
                None => error,
            }
        }
        None => fallback(),
    }
}

fn redact(url: &Url) -> String {
    let mut url = url.clone();
    url.set_query(None);
    url.set_fragment(None);
    let _ = url.set_password(None);
    let _ = url.set_username("");
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_strips_credentials_and_query() {
        let url = Url::parse("https://user:pw@api.example.com/v1/items?token=secret#x").unwrap();
        assert_eq!(redact(&url), "https://api.example.com/v1/items");
    }
}
