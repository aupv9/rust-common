use std::sync::Arc;

use common_error::{Error, Result};
use common_types::Validate;
use serde::Deserialize;

use crate::{ApiKeyConfig, ApiKeyVerifier, JwtConfig, JwtVerifier, Principal};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    pub jwt: Option<JwtConfig>,
    pub api_keys: Option<ApiKeyConfig>,
}

impl Validate for AuthConfig {
    fn validate(&self) -> Result<()> {
        if let Some(jwt) = &self.jwt {
            jwt.validate()?;
        }
        if let Some(api_keys) = &self.api_keys {
            api_keys.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Credential<'a> {
    Bearer(&'a str),
    ApiKey(&'a str),
}

#[derive(Debug, Clone, Default)]
pub struct Authenticator {
    jwt: Option<JwtVerifier>,
    api_keys: Option<Arc<ApiKeyVerifier>>,
}

impl Authenticator {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_config(config: &AuthConfig) -> Result<Self> {
        let mut authenticator = Self::new();
        if let Some(jwt) = &config.jwt {
            authenticator = authenticator.with_jwt(JwtVerifier::new(jwt)?);
        }
        if let Some(api_keys) = &config.api_keys {
            authenticator = authenticator.with_api_keys(ApiKeyVerifier::new(api_keys)?);
        }
        Ok(authenticator)
    }

    #[must_use]
    pub fn with_jwt(mut self, verifier: JwtVerifier) -> Self {
        self.jwt = Some(verifier);
        self
    }

    #[must_use]
    pub fn with_api_keys(mut self, verifier: ApiKeyVerifier) -> Self {
        self.api_keys = Some(Arc::new(verifier));
        self
    }

    #[must_use]
    pub fn jwt(&self) -> Option<&JwtVerifier> {
        self.jwt.as_ref()
    }

    pub async fn authenticate(&self, credential: Credential<'_>) -> Result<Principal> {
        match credential {
            Credential::Bearer(token) => match &self.jwt {
                Some(verifier) => verifier.verify(token).await,
                None => Err(unsupported()),
            },
            Credential::ApiKey(key) => match &self.api_keys {
                Some(verifier) => verifier.verify(key),
                None => Err(unsupported()),
            },
        }
    }
}

fn unsupported() -> Error {
    Error::unauthorized("unsupported credential").with_code("UNSUPPORTED_CREDENTIAL")
}

#[cfg(test)]
mod tests {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode, get_current_timestamp};
    use serde_json::json;

    use super::*;
    use crate::{ApiKeyEntry, AuthMethod, generate_api_key};

    const SECRET: &str = "0123456789abcdef0123456789abcdef";

    #[tokio::test]
    async fn dispatches_by_credential_type() {
        let (key, hash) = generate_api_key("sk").unwrap();
        let config: AuthConfig = serde_json::from_value(json!({
            "jwt": {
                "issuers": ["iss"],
                "audiences": ["aud"],
                "algorithms": ["HS256"],
                "keys": {"type": "secret", "secret": SECRET},
            },
            "api_keys": {
                "keys": [{"id": "k1", "hash": hash, "subject": "svc"}],
            },
        }))
        .unwrap();
        let auth = Authenticator::from_config(&config).unwrap();

        let token = encode(
            &Header::new(Algorithm::HS256),
            &json!({"sub": "u1", "iss": "iss", "aud": "aud", "exp": get_current_timestamp() + 60}),
            &EncodingKey::from_secret(SECRET.as_bytes()),
        )
        .unwrap();
        let user = auth.authenticate(Credential::Bearer(&token)).await.unwrap();
        assert_eq!(user.method(), AuthMethod::Jwt);

        let service = auth
            .authenticate(Credential::ApiKey(key.expose()))
            .await
            .unwrap();
        assert_eq!(service.subject(), "svc");
        assert_eq!(service.method(), AuthMethod::ApiKey);
    }

    #[tokio::test]
    async fn unconfigured_method_is_rejected() {
        let auth = Authenticator::new().with_api_keys(
            ApiKeyVerifier::new(&ApiKeyConfig {
                keys: vec![ApiKeyEntry {
                    id: "k".into(),
                    hash: crate::hash_api_key("x"),
                    subject: "s".into(),
                    tenant: None,
                    roles: vec![],
                    scopes: vec![],
                }],
            })
            .unwrap(),
        );
        let err = auth
            .authenticate(Credential::Bearer("token"))
            .await
            .unwrap_err();
        assert_eq!(err.code(), "UNSUPPORTED_CREDENTIAL");
    }
}
