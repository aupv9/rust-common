//! Run with: `cargo run -p common --example basic --features full`

use common::config::ConfigLoader;
use common::telemetry::{self, TelemetryConfig, tracing};
use common::utils::id::new_id;

#[derive(Debug, serde::Deserialize)]
struct AppConfig {
    name: String,
    #[serde(default)]
    telemetry: TelemetryConfig,
}

fn main() -> common::Result<()> {
    let cfg: AppConfig = ConfigLoader::new()
        .toml_str("name = \"demo\"\n[telemetry]\nformat = \"pretty\"")
        .file("config/local.toml")
        .env("APP")
        .load()?;

    telemetry::init(&cfg.telemetry)?;
    tracing::info!(app = %cfg.name, request_id = %new_id(), "started");
    Ok(())
}
