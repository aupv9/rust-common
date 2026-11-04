use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub host: IpAddr,
    pub port: u16,
    pub request_timeout_secs: u64,
    pub body_limit_bytes: usize,
    pub cors: CorsConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            port: 8080,
            request_timeout_secs: 30,
            body_limit_bytes: 2 * 1024 * 1024,
            cors: CorsConfig::default(),
        }
    }
}

impl ServerConfig {
    #[must_use]
    pub fn addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }

    #[must_use]
    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_secs)
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CorsConfig {
    pub allowed_origins: Vec<String>,
}
