use std::convert::Infallible;
use std::ops::Deref;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::Router;
use axum::extract::{FromRequestParts, OptionalFromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{Extensions, HeaderMap, HeaderName, HeaderValue, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use common_auth::{Authenticator, Credential, Principal, Requirement};
use common_error::{Error, ErrorKind};
use futures_util::future::{Either, Ready, ready};
use tower::{Layer, Service};

use crate::error::ApiError;

pub const API_KEY_HEADER: HeaderName = HeaderName::from_static("x-api-key");

#[derive(Clone, Copy)]
struct AuthInstalled;

pub fn with_auth<S>(router: Router<S>, authenticator: Authenticator) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router.layer(axum::middleware::from_fn_with_state(
        Arc::new(authenticator),
        authenticate,
    ))
}

async fn authenticate(
    State(authenticator): State<Arc<Authenticator>>,
    mut req: Request,
    next: Next,
) -> Response {
    let outcome = match credential(req.headers()) {
        Ok(Some(credential)) => authenticator.authenticate(credential).await.map(Some),
        Ok(None) => Ok(None),
        Err(err) => Err(err),
    };
    match outcome {
        Ok(principal) => {
            req.extensions_mut().insert(AuthInstalled);
            if let Some(principal) = principal {
                tracing::Span::current().record("enduser.id", principal.subject());
                req.extensions_mut().insert(principal);
            }
            next.run(req).await
        }
        Err(err) => AuthRejection::from(err).into_response(),
    }
}

fn credential(headers: &HeaderMap) -> Result<Option<Credential<'_>>, Error> {
    let malformed = || Error::unauthorized("malformed credentials").with_code("INVALID_TOKEN");
    if let Some(value) = headers.get(header::AUTHORIZATION) {
        let value = value.to_str().map_err(|_| malformed())?;
        if let Some((scheme, token)) = value.split_once(' ')
            && scheme.eq_ignore_ascii_case("bearer")
        {
            let token = token.trim();
            if token.is_empty() {
                return Err(malformed());
            }
            return Ok(Some(Credential::Bearer(token)));
        }
    }
    if let Some(value) = headers.get(API_KEY_HEADER) {
        let key = value.to_str().map_err(|_| malformed())?.trim();
        if key.is_empty() {
            return Err(malformed());
        }
        return Ok(Some(Credential::ApiKey(key)));
    }
    Ok(None)
}

fn unauthenticated(extensions: &Extensions) -> Error {
    if extensions.get::<AuthInstalled>().is_some() {
        Error::unauthorized("authentication required").with_code("AUTHENTICATION_REQUIRED")
    } else {
        Error::internal("route requires authentication but `with_auth` is not installed")
    }
}

#[derive(Debug)]
pub struct AuthRejection(Error);

impl From<Error> for AuthRejection {
    fn from(err: Error) -> Self {
        Self(err)
    }
}

impl IntoResponse for AuthRejection {
    fn into_response(self) -> Response {
        let challenge = match (self.0.kind(), self.0.code()) {
            (ErrorKind::Unauthorized, "INVALID_TOKEN" | "TOKEN_EXPIRED") => {
                Some(r#"Bearer error="invalid_token""#)
            }
            (ErrorKind::Unauthorized, _) => Some("Bearer"),
            _ => None,
        };
        let mut response = ApiError::new(self.0).into_response();
        if let Some(challenge) = challenge {
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static(challenge),
            );
        }
        response
    }
}

#[derive(Debug, Clone)]
pub struct Auth(pub Principal);

impl Deref for Auth {
    type Target = Principal;

    fn deref(&self) -> &Principal {
        &self.0
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Auth {
    type Rejection = AuthRejection;

    fn from_request_parts(
        parts: &mut Parts,
        _: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        std::future::ready(
            parts
                .extensions
                .get::<Principal>()
                .cloned()
                .map(Self)
                .ok_or_else(|| unauthenticated(&parts.extensions).into()),
        )
    }
}

impl<S: Send + Sync> OptionalFromRequestParts<S> for Auth {
    type Rejection = Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _: &S,
    ) -> impl Future<Output = Result<Option<Self>, Infallible>> + Send {
        std::future::ready(Ok(parts.extensions.get::<Principal>().cloned().map(Self)))
    }
}

#[must_use]
pub fn require(requirement: Requirement) -> RequireLayer {
    RequireLayer {
        requirement: Arc::new(requirement),
    }
}

#[derive(Debug, Clone)]
pub struct RequireLayer {
    requirement: Arc<Requirement>,
}

impl<S> Layer<S> for RequireLayer {
    type Service = RequireService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequireService {
            inner,
            requirement: Arc::clone(&self.requirement),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RequireService<S> {
    inner: S,
    requirement: Arc<Requirement>,
}

impl<S, B> Service<axum::http::Request<B>> for RequireService<S>
where
    S: Service<axum::http::Request<B>, Response = Response>,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Either<Ready<Result<Response, S::Error>>, S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: axum::http::Request<B>) -> Self::Future {
        let outcome = match req.extensions().get::<Principal>() {
            Some(principal) => self.requirement.check(principal),
            None => Err(unauthenticated(req.extensions())),
        };
        match outcome {
            Ok(()) => Either::Right(self.inner.call(req)),
            Err(err) => Either::Left(ready(Ok(AuthRejection(err).into_response()))),
        }
    }
}
