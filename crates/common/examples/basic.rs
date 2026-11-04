use common::config::{ConfigLoader, Validate};
use common::telemetry::{self, ServiceInfo, TelemetryConfig, tracing};
use common::types::{Id, NonEmptyString, Secret};
use common::{Error, OptionExt};

#[derive(Debug, serde::Deserialize)]
struct AppConfig {
    name: NonEmptyString,
    api_key: Secret<String>,
    #[serde(default)]
    telemetry: TelemetryConfig,
}

impl Validate for AppConfig {
    fn validate(&self) -> common::Result<()> {
        if self.api_key.expose().len() < 8 {
            return Err(Error::invalid_input("api_key too short"));
        }
        Ok(())
    }
}

struct User;

const DEFAULTS: &str = r#"
name = "demo"
api_key = "s3cr3t-key"

[telemetry]
format = "pretty"
"#;

fn find_user(id: Id<User>) -> common::Result<String> {
    None.or_not_found(format!("user {id}"))
}

fn main() -> common::Result<()> {
    let cfg: AppConfig = ConfigLoader::new()
        .toml_str(DEFAULTS)
        .with_env_profile("APP")
        .load_validated()?;

    let _guard = telemetry::init(&ServiceInfo::new("basic", "0.0.0"), &cfg.telemetry)?;
    tracing::info!(app = %cfg.name, ?cfg.api_key, "started");

    if let Err(err) = find_user(Id::new()) {
        tracing::warn!(code = err.code(), %err, "lookup failed");
    }
    Ok(())
}
