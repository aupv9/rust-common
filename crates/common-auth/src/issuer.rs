use std::fmt;
use std::time::Duration;

use aws_lc_rs::signature::{Ed25519KeyPair, KeyPair};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use common_error::{Error, Result};
use common_types::{Secret, Uuid, Validate};
use jsonwebtoken::jwk::{
    AlgorithmParameters, CommonParameters, EllipticCurve, Jwk, JwkSet, KeyAlgorithm,
    OctetKeyPairParameters, OctetKeyPairType, PublicKeyUse,
};
use jsonwebtoken::{
    Algorithm, AlgorithmFamily, EncodingKey, Header, encode, get_current_timestamp,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::Principal;
use crate::jwt::{MIN_SECRET_LEN, config_error};

const RESERVED: [&str; 11] = [
    "sub",
    "iss",
    "aud",
    "iat",
    "nbf",
    "exp",
    "jti",
    "tenant_id",
    "roles",
    "scope",
    "scp",
];

#[derive(Debug, Clone, Deserialize)]
pub struct IssuerConfig {
    pub issuer: String,
    pub audiences: Vec<String>,
    pub algorithm: Algorithm,
    pub key: SigningKey,
    #[serde(default)]
    pub key_id: Option<String>,
    #[serde(default = "default_ttl_secs")]
    pub ttl_secs: u64,
}

const fn default_ttl_secs() -> u64 {
    900
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum SigningKey {
    Secret { secret: Secret<String> },
    PrivateKeyPem { pem: Secret<String> },
}

impl Validate for IssuerConfig {
    fn validate(&self) -> Result<()> {
        if self.issuer.is_empty() {
            return Err(config_error("issuer must not be empty"));
        }
        if self.audiences.is_empty() {
            return Err(config_error("issuer audiences must not be empty"));
        }
        if self.ttl_secs == 0 {
            return Err(config_error("issuer ttl_secs must be positive"));
        }
        let hmac = self.algorithm.family() == AlgorithmFamily::Hmac;
        match &self.key {
            SigningKey::Secret { secret } => {
                if !hmac {
                    return Err(config_error(
                        "a shared secret only supports HS256, HS384 and HS512",
                    ));
                }
                if secret.expose().len() < MIN_SECRET_LEN {
                    return Err(config_error("jwt secret must be at least 32 bytes"));
                }
            }
            SigningKey::PrivateKeyPem { .. } => {
                if hmac {
                    return Err(config_error("a private key cannot sign HMAC algorithms"));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Serialize)]
pub struct IssuedToken {
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_in: u64,
    pub expires_at: u64,
}

impl fmt::Debug for IssuedToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IssuedToken")
            .field("access_token", &"***")
            .field("token_type", &self.token_type)
            .field("expires_in", &self.expires_in)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Clone)]
pub struct JwtIssuer {
    key: EncodingKey,
    header: Header,
    issuer: String,
    audience: Value,
    ttl: Duration,
}

impl fmt::Debug for JwtIssuer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JwtIssuer")
            .field("algorithm", &self.header.alg)
            .field("key_id", &self.header.kid)
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .field("ttl", &self.ttl)
            .finish_non_exhaustive()
    }
}

impl JwtIssuer {
    pub fn new(config: &IssuerConfig) -> Result<Self> {
        config.validate()?;
        let key = match &config.key {
            SigningKey::Secret { secret } => EncodingKey::from_secret(secret.expose().as_bytes()),
            SigningKey::PrivateKeyPem { pem } => {
                let bytes = pem.expose().as_bytes();
                match config.algorithm.family() {
                    AlgorithmFamily::Rsa => EncodingKey::from_rsa_pem(bytes),
                    AlgorithmFamily::Ec => EncodingKey::from_ec_pem(bytes),
                    AlgorithmFamily::Ed => EncodingKey::from_ed_pem(bytes),
                    AlgorithmFamily::Hmac => unreachable!("rejected by validate"),
                }
                .map_err(|e| config_error("invalid jwt private key").with_source(e))?
            }
        };
        let mut header = Header::new(config.algorithm);
        header.kid.clone_from(&config.key_id);
        let audience = match config.audiences.as_slice() {
            [single] => Value::String(single.clone()),
            many => Value::Array(many.iter().cloned().map(Value::String).collect()),
        };
        let issuer = Self {
            key,
            header,
            issuer: config.issuer.clone(),
            audience,
            ttl: Duration::from_secs(config.ttl_secs),
        };
        issuer.sign(&Map::new())?;
        Ok(issuer)
    }

    pub fn issue(&self, principal: &Principal) -> Result<IssuedToken> {
        self.issue_with_ttl(principal, self.ttl)
    }

    pub fn issue_with_ttl(&self, principal: &Principal, ttl: Duration) -> Result<IssuedToken> {
        let now = get_current_timestamp();
        let expires_in = ttl.as_secs().max(1);
        let expires_at = now + expires_in;

        let mut claims = principal.claims().clone();
        for name in RESERVED {
            claims.remove(name);
        }
        claims.insert("sub".into(), principal.subject().into());
        claims.insert("iss".into(), self.issuer.clone().into());
        claims.insert("aud".into(), self.audience.clone());
        claims.insert("iat".into(), now.into());
        claims.insert("nbf".into(), now.into());
        claims.insert("exp".into(), expires_at.into());
        claims.insert("jti".into(), Uuid::now_v7().to_string().into());
        if let Some(tenant) = principal.tenant() {
            claims.insert("tenant_id".into(), tenant.into());
        }
        if !principal.roles().is_empty() {
            claims.insert(
                "roles".into(),
                principal.roles().iter().cloned().collect::<Vec<_>>().into(),
            );
        }
        if !principal.scopes().is_empty() {
            let scope = principal
                .scopes()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(" ");
            claims.insert("scope".into(), scope.into());
        }

        Ok(IssuedToken {
            access_token: self.sign(&claims)?,
            token_type: "Bearer",
            expires_in,
            expires_at,
        })
    }

    pub fn jwks(&self) -> Result<JwkSet> {
        if self.header.alg.family() == AlgorithmFamily::Hmac {
            return Err(Error::invalid_input(
                "symmetric signing keys cannot be published",
            ));
        }
        let mut jwk = match self.header.alg.family() {
            AlgorithmFamily::Ed => ed25519_jwk(&self.key)?,
            _ => Jwk::from_encoding_key(&self.key, self.header.alg)
                .map_err(|e| Error::internal("failed to derive public key").with_source(e))?,
        };
        jwk.common.key_id.clone_from(&self.header.kid);
        jwk.common.public_key_use = Some(PublicKeyUse::Signature);
        Ok(JwkSet { keys: vec![jwk] })
    }

    fn sign(&self, claims: &Map<String, Value>) -> Result<String> {
        encode(&self.header, claims, &self.key)
            .map_err(|e| Error::internal("failed to sign token").with_source(e))
    }
}

fn ed25519_jwk(key: &EncodingKey) -> Result<Jwk> {
    let pair = Ed25519KeyPair::from_pkcs8(key.as_bytes())
        .map_err(|e| Error::internal("failed to derive public key").with_source(e))?;
    Ok(Jwk {
        common: CommonParameters {
            key_algorithm: Some(KeyAlgorithm::EdDSA),
            ..CommonParameters::default()
        },
        algorithm: AlgorithmParameters::OctetKeyPair(OctetKeyPairParameters {
            key_type: OctetKeyPairType::OctetKeyPair,
            curve: EllipticCurve::Ed25519,
            x: URL_SAFE_NO_PAD.encode(pair.public_key().as_ref()),
        }),
    })
}

#[cfg(test)]
mod tests {
    use base64::engine::general_purpose::STANDARD;
    use common_error::ErrorKind;

    use super::*;
    use crate::jwt::{JwtConfig, JwtVerifier, KeySource};
    use crate::{AuthMethod, ClaimMapping};

    const SECRET: &str = "0123456789abcdef0123456789abcdef";

    fn hmac_config() -> IssuerConfig {
        IssuerConfig {
            issuer: "https://orders.example.com".into(),
            audiences: vec!["orders-api".into()],
            algorithm: Algorithm::HS256,
            key: SigningKey::Secret {
                secret: Secret::new(SECRET.into()),
            },
            key_id: None,
            ttl_secs: 60,
        }
    }

    fn verifier_config(algorithm: Algorithm, keys: KeySource) -> JwtConfig {
        JwtConfig {
            issuers: vec!["https://orders.example.com".into()],
            audiences: vec!["orders-api".into()],
            algorithms: vec![algorithm],
            leeway_secs: 0,
            claims: ClaimMapping::default(),
            keys,
        }
    }

    fn principal() -> Principal {
        let mut extra = Map::new();
        extra.insert("email".into(), "a@example.com".into());
        extra.insert("exp".into(), 1.into());
        Principal::new("u1", AuthMethod::Jwt)
            .with_tenant("acme")
            .with_roles(["admin"])
            .with_scopes(["orders:read", "orders:write"])
            .with_claims(extra)
    }

    #[tokio::test]
    async fn hmac_roundtrip() {
        let issuer = JwtIssuer::new(&hmac_config()).unwrap();
        let token = issuer.issue(&principal()).unwrap();
        assert_eq!(token.token_type, "Bearer");
        assert_eq!(token.expires_in, 60);
        assert!(!format!("{token:?}").contains(&token.access_token));

        let verifier = JwtVerifier::new(&verifier_config(
            Algorithm::HS256,
            KeySource::Secret {
                secret: Secret::new(SECRET.into()),
            },
        ))
        .unwrap();
        let decoded = verifier.verify(&token.access_token).await.unwrap();
        assert_eq!(decoded.subject(), "u1");
        assert_eq!(decoded.tenant(), Some("acme"));
        assert!(decoded.has_role("admin"));
        assert!(decoded.has_scope("orders:write"));
        assert_eq!(decoded.claim("email"), Some(&Value::from("a@example.com")));
        assert_ne!(decoded.claim("exp"), Some(&Value::from(1)));

        assert!(matches!(
            issuer.jwks().unwrap_err().kind(),
            ErrorKind::InvalidInput
        ));
    }

    fn ed25519_pems() -> (String, String) {
        let rng = aws_lc_rs::rand::SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let private = pem("PRIVATE KEY", pkcs8.as_ref());
        let mut spki = vec![
            0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
        ];
        spki.extend_from_slice(pair.public_key().as_ref());
        (private, pem("PUBLIC KEY", &spki))
    }

    fn pem(label: &str, der: &[u8]) -> String {
        format!(
            "-----BEGIN {label}-----\n{}\n-----END {label}-----\n",
            STANDARD.encode(der)
        )
    }

    #[tokio::test]
    async fn eddsa_roundtrip_and_jwks() {
        let (private, public) = ed25519_pems();
        let issuer = JwtIssuer::new(&IssuerConfig {
            algorithm: Algorithm::EdDSA,
            key: SigningKey::PrivateKeyPem {
                pem: Secret::new(private),
            },
            key_id: Some("k1".into()),
            ..hmac_config()
        })
        .unwrap();
        let token = issuer.issue(&principal()).unwrap();

        let verifier = JwtVerifier::new(&verifier_config(
            Algorithm::EdDSA,
            KeySource::PublicKeyPem { pem: public },
        ))
        .unwrap();
        assert_eq!(
            verifier
                .verify(&token.access_token)
                .await
                .unwrap()
                .subject(),
            "u1"
        );

        let jwks = issuer.jwks().unwrap();
        assert_eq!(jwks.keys.len(), 1);
        assert_eq!(jwks.keys[0].common.key_id.as_deref(), Some("k1"));
    }

    #[test]
    fn config_validation() {
        let mut cfg = hmac_config();
        cfg.algorithm = Algorithm::RS256;
        assert!(JwtIssuer::new(&cfg).is_err());

        let mut cfg = hmac_config();
        cfg.audiences.clear();
        assert!(JwtIssuer::new(&cfg).is_err());

        let cfg = IssuerConfig {
            algorithm: Algorithm::EdDSA,
            key: SigningKey::PrivateKeyPem {
                pem: Secret::new("garbage".into()),
            },
            ..hmac_config()
        };
        assert_eq!(JwtIssuer::new(&cfg).unwrap_err().kind(), ErrorKind::Config);
    }
}
