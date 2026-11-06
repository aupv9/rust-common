use std::fmt;
use std::sync::Arc;

use common_error::{Error, ErrorKind, Result};
use common_types::{Secret, Validate};
use jsonwebtoken::errors::ErrorKind as JwtErrorKind;
use jsonwebtoken::{Algorithm, AlgorithmFamily, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::claims::ClaimMapping;
use crate::{AuthMethod, Principal};

const MAX_TOKEN_LEN: usize = 16 * 1024;
pub(crate) const MIN_SECRET_LEN: usize = 32;

#[derive(Debug, Clone, Deserialize)]
pub struct JwtConfig {
    pub issuers: Vec<String>,
    pub audiences: Vec<String>,
    #[serde(default = "default_algorithms")]
    pub algorithms: Vec<Algorithm>,
    #[serde(default = "default_leeway_secs")]
    pub leeway_secs: u64,
    #[serde(default)]
    pub claims: ClaimMapping,
    pub keys: KeySource,
}

fn default_algorithms() -> Vec<Algorithm> {
    vec![Algorithm::RS256]
}

const fn default_leeway_secs() -> u64 {
    30
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum KeySource {
    #[cfg(feature = "jwks")]
    Jwks {
        url: String,
        #[serde(default = "default_cache_secs")]
        cache_secs: u64,
        #[serde(default = "default_min_refresh_secs")]
        min_refresh_secs: u64,
    },
    Secret {
        secret: Secret<String>,
    },
    PublicKeyPem {
        pem: String,
    },
}

#[cfg(feature = "jwks")]
const fn default_cache_secs() -> u64 {
    300
}

#[cfg(feature = "jwks")]
const fn default_min_refresh_secs() -> u64 {
    30
}

pub(crate) fn config_error(message: impl Into<std::borrow::Cow<'static, str>>) -> Error {
    Error::new(ErrorKind::Config, message)
}

impl Validate for JwtConfig {
    fn validate(&self) -> Result<()> {
        if self.issuers.is_empty() {
            return Err(config_error("jwt.issuers must not be empty"));
        }
        if self.audiences.is_empty() {
            return Err(config_error("jwt.audiences must not be empty"));
        }
        let Some(first) = self.algorithms.first() else {
            return Err(config_error("jwt.algorithms must not be empty"));
        };
        let family = first.family();
        if self.algorithms.iter().any(|a| a.family() != family) {
            return Err(config_error(
                "jwt.algorithms must all belong to one key family",
            ));
        }
        match &self.keys {
            KeySource::Secret { secret } => {
                if family != AlgorithmFamily::Hmac {
                    return Err(config_error(
                        "a shared secret only supports HS256, HS384 and HS512",
                    ));
                }
                if secret.expose().len() < MIN_SECRET_LEN {
                    return Err(config_error("jwt secret must be at least 32 bytes"));
                }
            }
            KeySource::PublicKeyPem { .. } => {
                if family == AlgorithmFamily::Hmac {
                    return Err(config_error("a public key cannot verify HMAC algorithms"));
                }
            }
            #[cfg(feature = "jwks")]
            KeySource::Jwks {
                url,
                cache_secs,
                min_refresh_secs,
            } => {
                if family == AlgorithmFamily::Hmac {
                    return Err(config_error("JWKS keys cannot verify HMAC algorithms"));
                }
                if !(url.starts_with("https://") || url.starts_with("http://")) {
                    return Err(config_error("jwt.keys.url must be an http(s) URL"));
                }
                if min_refresh_secs > cache_secs {
                    return Err(config_error(
                        "jwt.keys.min_refresh_secs must not exceed cache_secs",
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct JwtVerifier {
    inner: Arc<Inner>,
}

struct Inner {
    validation: Validation,
    claims: ClaimMapping,
    keys: Keys,
}

enum Keys {
    Static(Arc<DecodingKey>),
    #[cfg(feature = "jwks")]
    Jwks(Box<crate::jwks::JwksCache>),
}

impl fmt::Debug for JwtVerifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let keys = match &self.inner.keys {
            Keys::Static(_) => "static",
            #[cfg(feature = "jwks")]
            Keys::Jwks(_) => "jwks",
        };
        f.debug_struct("JwtVerifier")
            .field("algorithms", &self.inner.validation.algorithms)
            .field("issuers", &self.inner.validation.iss)
            .field("audiences", &self.inner.validation.aud)
            .field("keys", &keys)
            .finish_non_exhaustive()
    }
}

impl JwtVerifier {
    pub fn new(config: &JwtConfig) -> Result<Self> {
        config.validate()?;
        let keys = match &config.keys {
            KeySource::Secret { secret } => Keys::Static(Arc::new(DecodingKey::from_secret(
                secret.expose().as_bytes(),
            ))),
            KeySource::PublicKeyPem { pem } => {
                Keys::Static(Arc::new(public_key(pem, config.algorithms[0].family())?))
            }
            #[cfg(feature = "jwks")]
            KeySource::Jwks {
                url,
                cache_secs,
                min_refresh_secs,
            } => Keys::Jwks(Box::new(crate::jwks::JwksCache::new(
                url.clone(),
                std::time::Duration::from_secs(*cache_secs),
                std::time::Duration::from_secs(*min_refresh_secs),
            )?)),
        };
        Ok(Self::from_parts(config, keys))
    }

    #[cfg(feature = "jwks")]
    pub fn with_http_client(
        config: &JwtConfig,
        client: common_http_client::HttpClient,
    ) -> Result<Self> {
        config.validate()?;
        match &config.keys {
            KeySource::Jwks {
                url,
                cache_secs,
                min_refresh_secs,
            } => {
                let cache = crate::jwks::JwksCache::with_client(
                    client,
                    url.clone(),
                    std::time::Duration::from_secs(*cache_secs),
                    std::time::Duration::from_secs(*min_refresh_secs),
                );
                Ok(Self::from_parts(config, Keys::Jwks(Box::new(cache))))
            }
            _ => Self::new(config),
        }
    }

    fn from_parts(config: &JwtConfig, keys: Keys) -> Self {
        let mut validation = Validation::new(config.algorithms[0]);
        validation.algorithms.clone_from(&config.algorithms);
        validation.set_issuer(&config.issuers);
        validation.set_audience(&config.audiences);
        validation.set_required_spec_claims(&["exp", "iss", "aud"]);
        validation.leeway = config.leeway_secs;
        validation.validate_nbf = true;
        Self {
            inner: Arc::new(Inner {
                validation,
                claims: config.claims.clone(),
                keys,
            }),
        }
    }

    #[cfg_attr(
        not(feature = "jwks"),
        allow(clippy::unused_async, clippy::unused_async_trait_impl)
    )]
    pub async fn preload(&self) -> Result<()> {
        match &self.inner.keys {
            Keys::Static(_) => Ok(()),
            #[cfg(feature = "jwks")]
            Keys::Jwks(cache) => cache.preload().await,
        }
    }

    #[cfg_attr(
        not(feature = "jwks"),
        allow(clippy::unused_async, clippy::unused_async_trait_impl)
    )]
    pub async fn verify(&self, token: &str) -> Result<Principal> {
        if token.len() > MAX_TOKEN_LEN {
            return Err(invalid_token("token too large"));
        }
        let header = decode_header(token).map_err(token_error)?;
        if !self.inner.validation.algorithms.contains(&header.alg) {
            return Err(invalid_token("algorithm not allowed"));
        }
        let key = match &self.inner.keys {
            Keys::Static(key) => Arc::clone(key),
            #[cfg(feature = "jwks")]
            Keys::Jwks(cache) => cache.key(header.kid.as_deref(), header.alg).await?,
        };
        let data = decode::<Map<String, Value>>(token, &key, &self.inner.validation)
            .map_err(token_error)?;
        self.inner.claims.to_principal(data.claims, AuthMethod::Jwt)
    }
}

fn public_key(pem: &str, family: AlgorithmFamily) -> Result<DecodingKey> {
    let bytes = pem.as_bytes();
    let key = match family {
        AlgorithmFamily::Rsa => DecodingKey::from_rsa_pem(bytes),
        AlgorithmFamily::Ec => DecodingKey::from_ec_pem(bytes),
        AlgorithmFamily::Ed => DecodingKey::from_ed_pem(bytes),
        AlgorithmFamily::Hmac => {
            return Err(config_error("a public key cannot verify HMAC algorithms"));
        }
    };
    key.map_err(|e| config_error("invalid jwt public key").with_source(e))
}

pub(crate) fn invalid_token(reason: &'static str) -> Error {
    Error::unauthorized("invalid token")
        .with_code("INVALID_TOKEN")
        .with_source(Error::unauthorized(reason))
}

fn token_error(err: jsonwebtoken::errors::Error) -> Error {
    match err.kind() {
        JwtErrorKind::ExpiredSignature => {
            Error::unauthorized("token expired").with_code("TOKEN_EXPIRED")
        }
        _ => Error::unauthorized("invalid token")
            .with_code("INVALID_TOKEN")
            .with_source(err),
    }
}

#[cfg(test)]
mod tests {
    use jsonwebtoken::{EncodingKey, Header, encode, get_current_timestamp};
    use serde_json::json;

    use super::*;

    const SECRET: &str = "0123456789abcdef0123456789abcdef";

    fn config() -> JwtConfig {
        JwtConfig {
            issuers: vec!["https://auth.example.com".into()],
            audiences: vec!["orders-api".into()],
            algorithms: vec![Algorithm::HS256],
            leeway_secs: 0,
            claims: ClaimMapping::default(),
            keys: KeySource::Secret {
                secret: Secret::new(SECRET.into()),
            },
        }
    }

    fn sign(claims: &Value) -> String {
        encode(
            &Header::new(Algorithm::HS256),
            claims,
            &EncodingKey::from_secret(SECRET.as_bytes()),
        )
        .unwrap()
    }

    fn claims(exp_offset: i64) -> Value {
        let now = get_current_timestamp().cast_signed();
        json!({
            "sub": "u1",
            "iss": "https://auth.example.com",
            "aud": "orders-api",
            "exp": now + exp_offset,
            "tenant_id": "acme",
            "roles": ["admin"],
            "scope": "orders:read",
        })
    }

    #[tokio::test]
    async fn verifies_valid_token() {
        let verifier = JwtVerifier::new(&config()).unwrap();
        let principal = verifier.verify(&sign(&claims(60))).await.unwrap();
        assert_eq!(principal.subject(), "u1");
        assert_eq!(principal.tenant(), Some("acme"));
        assert!(principal.has_role("admin"));
        assert!(principal.has_scope("orders:read"));
        assert_eq!(principal.method(), AuthMethod::Jwt);
    }

    #[tokio::test]
    async fn expired_token_has_distinct_code() {
        let verifier = JwtVerifier::new(&config()).unwrap();
        let err = verifier.verify(&sign(&claims(-60))).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unauthorized);
        assert_eq!(err.code(), "TOKEN_EXPIRED");
    }

    #[tokio::test]
    async fn rejects_wrong_audience_issuer_and_signature() {
        let verifier = JwtVerifier::new(&config()).unwrap();

        let mut wrong_aud = claims(60);
        wrong_aud["aud"] = json!("billing-api");
        let mut wrong_iss = claims(60);
        wrong_iss["iss"] = json!("https://evil.example.com");
        let mut missing_aud = claims(60);
        missing_aud.as_object_mut().unwrap().remove("aud");
        let forged = encode(
            &Header::new(Algorithm::HS256),
            &claims(60),
            &EncodingKey::from_secret(b"another-secret-another-secret-xx"),
        )
        .unwrap();

        for token in [
            sign(&wrong_aud),
            sign(&wrong_iss),
            sign(&missing_aud),
            forged,
            "not.a.jwt".to_owned(),
        ] {
            let err = verifier.verify(&token).await.unwrap_err();
            assert_eq!(err.code(), "INVALID_TOKEN", "token {token}");
        }
    }

    #[tokio::test]
    async fn rejects_algorithm_outside_allow_list() {
        let verifier = JwtVerifier::new(&config()).unwrap();
        let token = encode(
            &Header::new(Algorithm::HS512),
            &claims(60),
            &EncodingKey::from_secret(SECRET.as_bytes()),
        )
        .unwrap();
        assert_eq!(
            verifier.verify(&token).await.unwrap_err().code(),
            "INVALID_TOKEN"
        );
    }

    #[test]
    fn config_validation() {
        let mut cfg = config();
        cfg.audiences.clear();
        assert!(cfg.validate().is_err());

        let mut cfg = config();
        cfg.keys = KeySource::Secret {
            secret: Secret::new("short".into()),
        };
        assert!(cfg.validate().is_err());

        let mut cfg = config();
        cfg.algorithms = vec![Algorithm::RS256];
        assert!(cfg.validate().is_err());

        let mut cfg = config();
        cfg.algorithms = vec![Algorithm::HS256, Algorithm::RS256];
        assert!(cfg.validate().is_err());

        let mut cfg = config();
        cfg.keys = KeySource::PublicKeyPem { pem: String::new() };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn deserializes_from_toml_like_json() {
        let cfg: JwtConfig = serde_json::from_value(json!({
            "issuers": ["https://auth.example.com"],
            "audiences": ["orders-api"],
            "algorithms": ["HS256"],
            "keys": {"type": "secret", "secret": SECRET},
        }))
        .unwrap();
        assert_eq!(cfg.leeway_secs, 30);
        assert!(cfg.validate().is_ok());
        assert!(!format!("{cfg:?}").contains(SECRET));
    }
}
