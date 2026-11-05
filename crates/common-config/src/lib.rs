use std::path::Path;

use common_error::{Error, ErrorKind, Result};
use config::builder::DefaultState;
use config::{Config, ConfigBuilder, Environment, File, FileFormat};
use serde::de::DeserializeOwned;

pub use common_types::Validate;

pub const DEFAULT_DIR: &str = "config";
pub const DEFAULT_PROFILE: &str = "local";

#[derive(Debug, Default)]
pub struct ConfigLoader {
    builder: ConfigBuilder<DefaultState>,
    profile: Option<String>,
}

impl ConfigLoader {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn from_env(prefix: &str) -> Self {
        Self::new().with_env_profile(prefix)
    }

    #[must_use]
    pub fn with_env_profile(self, prefix: &str) -> Self {
        let profile = std::env::var(format!("{prefix}_ENV"))
            .ok()
            .filter(|p| !p.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_PROFILE.to_owned());
        self.with_profile(DEFAULT_DIR, &profile, prefix)
    }

    #[must_use]
    pub fn with_profile(self, dir: impl AsRef<Path>, profile: &str, env_prefix: &str) -> Self {
        let dir = dir.as_ref();
        let mut loader = self
            .file(dir.join("default.toml"))
            .file(dir.join(format!("{profile}.toml")))
            .env(env_prefix);
        loader.profile = Some(profile.to_owned());
        loader
    }

    #[must_use]
    pub fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }

    #[must_use]
    pub fn file(self, path: impl AsRef<Path>) -> Self {
        self.add_file(path.as_ref(), false)
    }

    #[must_use]
    pub fn required_file(self, path: impl AsRef<Path>) -> Self {
        self.add_file(path.as_ref(), true)
    }

    #[must_use]
    pub fn toml_str(mut self, toml: &str) -> Self {
        self.builder = self
            .builder
            .add_source(File::from_str(toml, FileFormat::Toml));
        self
    }

    #[must_use]
    pub fn env(mut self, prefix: &str) -> Self {
        self.builder = self.builder.add_source(
            Environment::with_prefix(prefix)
                .prefix_separator("__")
                .separator("__"),
        );
        self
    }

    pub fn load<T: DeserializeOwned>(self) -> Result<T> {
        self.builder
            .build()
            .and_then(Config::try_deserialize)
            .map_err(|e| {
                Error::new(ErrorKind::Config, "failed to load configuration").with_source(e)
            })
    }

    pub fn load_validated<T: DeserializeOwned + Validate>(self) -> Result<T> {
        let cfg: T = self.load()?;
        cfg.validate()
            .map_err(|e| Error::new(ErrorKind::Config, "invalid configuration").with_source(e))?;
        Ok(cfg)
    }

    fn add_file(mut self, path: &Path, required: bool) -> Self {
        self.builder = self.builder.add_source(File::from(path).required(required));
        self
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct Db {
        url: String,
        pool: u32,
    }

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct App {
        name: String,
        db: Db,
    }

    impl Validate for App {
        fn validate(&self) -> Result<()> {
            if self.db.pool == 0 {
                return Err(Error::invalid_input("db.pool must be > 0"));
            }
            Ok(())
        }
    }

    const BASE: &str = "name = \"svc\"\n[db]\nurl = \"a\"\npool = 1";

    #[test]
    fn later_sources_override_earlier() {
        let app: App = ConfigLoader::new()
            .toml_str(BASE)
            .toml_str("[db]\npool = 5")
            .load()
            .unwrap();
        assert_eq!(
            app.db,
            Db {
                url: "a".into(),
                pool: 5
            }
        );
    }

    #[test]
    fn missing_optional_file_is_ignored() {
        let app: App = ConfigLoader::new()
            .toml_str(BASE)
            .file("nope.toml")
            .load()
            .unwrap();
        assert_eq!(app.name, "svc");
    }

    #[test]
    fn missing_required_file_is_config_error() {
        let err = ConfigLoader::new()
            .required_file("nope.toml")
            .load::<App>()
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Config);
    }

    #[test]
    fn profile_file_overrides_default_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("default.toml"), BASE).unwrap();
        fs::write(dir.path().join("prod.toml"), "[db]\nurl = \"prod\"").unwrap();

        let loader = ConfigLoader::new().with_profile(dir.path(), "prod", "COMMON_CONFIG_TEST");
        assert_eq!(loader.profile(), Some("prod"));
        let app: App = loader.load().unwrap();
        assert_eq!(
            app.db,
            Db {
                url: "prod".into(),
                pool: 1
            }
        );
    }

    #[test]
    fn validation_failure_is_config_error() {
        let err = ConfigLoader::new()
            .toml_str(BASE)
            .toml_str("[db]\npool = 0")
            .load_validated::<App>()
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Config);
    }
}
