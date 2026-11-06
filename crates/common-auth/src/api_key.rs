use std::fmt;

use aws_lc_rs::{constant_time, digest, rand};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use common_error::{Error, Result};
use common_types::{Secret, Validate};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::jwt::config_error;
use crate::{AuthMethod, Principal};

const DIGEST_LEN: usize = 32;
const KEY_BYTES: usize = 32;
const MAX_KEY_LEN: usize = 512;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ApiKeyConfig {
    pub keys: Vec<ApiKeyEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiKeyEntry {
    pub id: String,
    pub hash: String,
    pub subject: String,
    #[serde(default)]
    pub tenant: Option<String>,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

impl Validate for ApiKeyConfig {
    fn validate(&self) -> Result<()> {
        for entry in &self.keys {
            if entry.id.is_empty() || entry.subject.is_empty() {
                return Err(config_error("api key id and subject must not be empty"));
            }
            decode_hash(&entry.hash).map_err(|e| {
                config_error(format!("api key `{}` has an invalid hash", entry.id)).with_source(e)
            })?;
        }
        let mut ids: Vec<_> = self.keys.iter().map(|e| e.id.as_str()).collect();
        ids.sort_unstable();
        if ids.windows(2).any(|w| w[0] == w[1]) {
            return Err(config_error("api key ids must be unique"));
        }
        Ok(())
    }
}

pub struct ApiKeyVerifier {
    entries: Vec<([u8; DIGEST_LEN], Principal)>,
}

impl fmt::Debug for ApiKeyVerifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiKeyVerifier")
            .field("keys", &self.entries.len())
            .finish()
    }
}

impl ApiKeyVerifier {
    pub fn new(config: &ApiKeyConfig) -> Result<Self> {
        config.validate()?;
        let entries = config
            .keys
            .iter()
            .map(|entry| {
                let mut claims = Map::new();
                claims.insert("api_key_id".to_owned(), Value::String(entry.id.clone()));
                let principal = Principal::new(entry.subject.clone(), AuthMethod::ApiKey)
                    .with_roles(entry.roles.iter().cloned())
                    .with_scopes(entry.scopes.iter().cloned())
                    .with_claims(claims);
                let principal = match &entry.tenant {
                    Some(tenant) => principal.with_tenant(tenant.clone()),
                    None => principal,
                };
                Ok((decode_hash(&entry.hash)?, principal))
            })
            .collect::<Result<_>>()?;
        Ok(Self { entries })
    }

    pub fn verify(&self, key: &str) -> Result<Principal> {
        let invalid = || Error::unauthorized("invalid api key").with_code("INVALID_API_KEY");
        if key.is_empty() || key.len() > MAX_KEY_LEN {
            return Err(invalid());
        }
        let presented = digest::digest(&digest::SHA256, key.as_bytes());
        let mut matched = None;
        for (hash, principal) in &self.entries {
            if constant_time::verify_slices_are_equal(hash, presented.as_ref()).is_ok() {
                matched = Some(principal);
            }
        }
        matched.cloned().ok_or_else(invalid)
    }
}

#[must_use]
pub fn hash_api_key(key: &str) -> String {
    hex(digest::digest(&digest::SHA256, key.as_bytes()).as_ref())
}

pub fn generate_api_key(prefix: &str) -> Result<(Secret<String>, String)> {
    let mut bytes = [0u8; KEY_BYTES];
    rand::fill(&mut bytes).map_err(|_| Error::internal("system random generator failed"))?;
    let body = URL_SAFE_NO_PAD.encode(bytes);
    let key = if prefix.is_empty() {
        body
    } else {
        format!("{prefix}_{body}")
    };
    let hash = hash_api_key(&key);
    Ok((Secret::new(key), hash))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(char::from(DIGITS[usize::from(b >> 4)]));
        out.push(char::from(DIGITS[usize::from(b & 0x0f)]));
    }
    out
}

fn decode_hash(hash: &str) -> Result<[u8; DIGEST_LEN]> {
    let invalid = || Error::invalid_input("expected 64 hex characters (SHA-256)");
    let bytes = hash.as_bytes();
    if bytes.len() != DIGEST_LEN * 2 {
        return Err(invalid());
    }
    let nibble = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    };
    let mut out = [0u8; DIGEST_LEN];
    for (i, pair) in bytes.as_chunks::<2>().0.iter().enumerate() {
        out[i] =
            (nibble(pair[0]).ok_or_else(invalid)? << 4) | nibble(pair[1]).ok_or_else(invalid)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use common_error::ErrorKind;

    use super::*;

    fn verifier(hash: String) -> ApiKeyVerifier {
        ApiKeyVerifier::new(&ApiKeyConfig {
            keys: vec![ApiKeyEntry {
                id: "billing-worker".into(),
                hash,
                subject: "svc:billing".into(),
                tenant: None,
                roles: vec!["service".into()],
                scopes: vec!["invoices:write".into()],
            }],
        })
        .unwrap()
    }

    #[test]
    fn generated_key_verifies() {
        let (key, hash) = generate_api_key("sk").unwrap();
        assert!(key.expose().starts_with("sk_"));
        assert_eq!(hash.len(), 64);

        let principal = verifier(hash).verify(key.expose()).unwrap();
        assert_eq!(principal.subject(), "svc:billing");
        assert_eq!(principal.method(), AuthMethod::ApiKey);
        assert!(principal.has_scope("invoices:write"));
        assert_eq!(
            principal.claim("api_key_id"),
            Some(&Value::String("billing-worker".into()))
        );
    }

    #[test]
    fn wrong_key_is_unauthorized() {
        let (_, hash) = generate_api_key("sk").unwrap();
        let err = verifier(hash).verify("sk_nope").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unauthorized);
        assert_eq!(err.code(), "INVALID_API_KEY");
    }

    #[test]
    fn known_vector() {
        assert_eq!(
            hash_api_key("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            decode_hash(&hash_api_key("abc").to_uppercase()).unwrap(),
            decode_hash(&hash_api_key("abc")).unwrap()
        );
    }

    #[test]
    fn config_rejects_bad_hash_and_duplicate_ids() {
        let entry = |id: &str, hash: &str| ApiKeyEntry {
            id: id.into(),
            hash: hash.into(),
            subject: "s".into(),
            tenant: None,
            roles: vec![],
            scopes: vec![],
        };
        let good = hash_api_key("k");
        assert!(
            ApiKeyConfig {
                keys: vec![entry("a", "zz")]
            }
            .validate()
            .is_err()
        );
        assert!(
            ApiKeyConfig {
                keys: vec![entry("a", &good), entry("a", &good)]
            }
            .validate()
            .is_err()
        );
    }
}
