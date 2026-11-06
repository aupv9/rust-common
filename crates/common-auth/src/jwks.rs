use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard};
use std::time::{Duration, Instant};

use common_error::{Error, ErrorKind, Result};
use common_http_client::{HttpClient, HttpClientConfig};
use jsonwebtoken::jwk::{AlgorithmParameters, JwkSet, PublicKeyUse};
use jsonwebtoken::{Algorithm, AlgorithmFamily, DecodingKey};
use tokio::sync::Mutex;

use crate::jwt::invalid_token;

pub(crate) struct JwksCache {
    client: HttpClient,
    url: String,
    max_age: Duration,
    min_refresh: Duration,
    state: RwLock<State>,
    refresh: Mutex<()>,
}

#[derive(Default)]
struct State {
    keys: Vec<CachedKey>,
    fetched_at: Option<Instant>,
    attempted_at: Option<Instant>,
    last_failed: bool,
}

struct CachedKey {
    kid: Option<String>,
    alg: Option<Algorithm>,
    family: AlgorithmFamily,
    key: Arc<DecodingKey>,
}

impl State {
    fn is_fresh(&self, max_age: Duration) -> bool {
        self.fetched_at.is_some_and(|t| t.elapsed() < max_age)
    }

    fn find(&self, kid: Option<&str>, alg: Algorithm) -> Option<Arc<DecodingKey>> {
        let mut compatible = self
            .keys
            .iter()
            .filter(|k| k.family == alg.family() && k.alg.is_none_or(|a| a == alg));
        let found = match kid {
            Some(kid) => compatible.find(|k| k.kid.as_deref() == Some(kid)),
            None => match (compatible.next(), compatible.next()) {
                (Some(only), None) => Some(only),
                _ => None,
            },
        };
        found.map(|k| Arc::clone(&k.key))
    }
}

impl JwksCache {
    pub(crate) fn new(url: String, max_age: Duration, min_refresh: Duration) -> Result<Self> {
        let config = HttpClientConfig {
            timeout_secs: 5,
            connect_timeout_secs: 3,
            ..HttpClientConfig::default()
        };
        let client = HttpClient::new("jwks", &config)?;
        Ok(Self::with_client(client, url, max_age, min_refresh))
    }

    pub(crate) fn with_client(
        client: HttpClient,
        url: String,
        max_age: Duration,
        min_refresh: Duration,
    ) -> Self {
        Self {
            client,
            url,
            max_age,
            min_refresh,
            state: RwLock::new(State::default()),
            refresh: Mutex::new(()),
        }
    }

    fn read(&self) -> RwLockReadGuard<'_, State> {
        self.state.read().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) async fn preload(&self) -> Result<()> {
        let _guard = self.refresh.lock().await;
        let fetched = self.fetch().await;
        self.store(fetched)
    }

    pub(crate) async fn key(&self, kid: Option<&str>, alg: Algorithm) -> Result<Arc<DecodingKey>> {
        if let Some(key) = self.fresh_key(kid, alg) {
            return Ok(key);
        }

        let _guard = self.refresh.lock().await;
        if let Some(key) = self.fresh_key(kid, alg) {
            return Ok(key);
        }

        let recently_attempted = self
            .read()
            .attempted_at
            .is_some_and(|t| t.elapsed() < self.min_refresh);
        if !recently_attempted {
            let fetched = self.fetch().await;
            let _ = self.store(fetched);
        }

        let state = self.read();
        if let Some(key) = state.find(kid, alg) {
            return Ok(key);
        }
        if state.last_failed {
            return Err(Error::new(
                ErrorKind::Unavailable,
                "signing keys are temporarily unavailable",
            )
            .with_code("JWKS_UNAVAILABLE"));
        }
        Err(invalid_token("unknown signing key"))
    }

    fn fresh_key(&self, kid: Option<&str>, alg: Algorithm) -> Option<Arc<DecodingKey>> {
        let state = self.read();
        if state.is_fresh(self.max_age) {
            state.find(kid, alg)
        } else {
            None
        }
    }

    fn store(&self, fetched: Result<Vec<CachedKey>>) -> Result<()> {
        let mut state = self.state.write().unwrap_or_else(PoisonError::into_inner);
        let now = Instant::now();
        state.attempted_at = Some(now);
        match fetched {
            Ok(keys) => {
                state.keys = keys;
                state.fetched_at = Some(now);
                state.last_failed = false;
                Ok(())
            }
            Err(err) => {
                state.last_failed = true;
                Err(err)
            }
        }
    }

    async fn fetch(&self) -> Result<Vec<CachedKey>> {
        let set: JwkSet = self
            .client
            .send_json(self.client.get(&self.url))
            .await
            .map_err(|e| {
                tracing::warn!(url = %self.url, error = %e, "failed to fetch JWKS");
                Error::new(ErrorKind::Unavailable, "failed to fetch signing keys")
                    .with_code("JWKS_UNAVAILABLE")
                    .with_source(e)
            })?;
        let keys = parse(&set);
        tracing::debug!(url = %self.url, keys = keys.len(), "JWKS refreshed");
        Ok(keys)
    }
}

fn parse(set: &JwkSet) -> Vec<CachedKey> {
    set.keys
        .iter()
        .filter_map(|jwk| {
            if matches!(jwk.common.public_key_use, Some(ref u) if *u != PublicKeyUse::Signature) {
                return None;
            }
            if matches!(
                jwk.algorithm,
                AlgorithmParameters::OctetKey(_) | AlgorithmParameters::Other(_)
            ) {
                return None;
            }
            let alg = match jwk.common.key_algorithm {
                Some(ka) => Some(ka.to_string().parse::<Algorithm>().ok()?),
                None => None,
            };
            let key = DecodingKey::from_jwk(jwk).ok()?;
            Some(CachedKey {
                kid: jwk.common.key_id.clone(),
                alg,
                family: key.family(),
                key: Arc::new(key),
            })
        })
        .collect()
}
