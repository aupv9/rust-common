//! Layered configuration loading.
//!
//! Sources are merged in the order they are added; later sources override earlier ones.
//!
//! ```
//! use common_config::ConfigLoader;
//!
//! #[derive(serde::Deserialize)]
//! struct AppConfig { port: u16 }
//!
//! let cfg: AppConfig = ConfigLoader::new()
//!     .toml_str("port = 8080")
//!     .file("config/local.toml")   // optional, ignored if missing
//!     .env("APP")                  // e.g. APP__PORT=9000
//!     .load()
//!     .unwrap();
//! assert_eq!(cfg.port, 8080);
//! ```

use std::path::Path;

use common_error::{Error, ErrorKind, Result};
use config::builder::DefaultState;
use config::{Config, ConfigBuilder, Environment, File, FileFormat};
use serde::de::DeserializeOwned;

/// Builder for layered configuration.
#[derive(Debug, Default)]
pub struct ConfigLoader {
    builder: ConfigBuilder<DefaultState>,
}

impl ConfigLoader {
    /// Creates an empty loader.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an optional file source; missing files are ignored.
    /// The format is inferred from the extension.
    #[must_use]
    pub fn file(self, path: impl AsRef<Path>) -> Self {
        self.add_file(path.as_ref(), false)
    }

    /// Adds a file source that must exist.
    #[must_use]
    pub fn required_file(self, path: impl AsRef<Path>) -> Self {
        self.add_file(path.as_ref(), true)
    }

    /// Adds inline TOML (useful for defaults and tests).
    #[must_use]
    pub fn toml_str(mut self, toml: &str) -> Self {
        self.builder = self
            .builder
            .add_source(File::from_str(toml, FileFormat::Toml));
        self
    }

    /// Adds environment variables with the given prefix.
    /// Nested keys use `__` as separator: `APP__DB__URL` -> `db.url`.
    #[must_use]
    pub fn env(mut self, prefix: &str) -> Self {
        self.builder = self.builder.add_source(
            Environment::with_prefix(prefix)
                .prefix_separator("__")
                .separator("__"),
        );
        self
    }

    /// Builds and deserializes the merged configuration.
    ///
    /// # Errors
    ///
    /// Returns an [`ErrorKind::Config`] error if a required source is missing,
    /// a source fails to parse, or deserialization into `T` fails.
    pub fn load<T: DeserializeOwned>(self) -> Result<T> {
        self.builder
            .build()
            .and_then(Config::try_deserialize)
            .map_err(|e| {
                Error::new(ErrorKind::Config, "failed to load configuration").with_source(e)
            })
    }

    fn add_file(mut self, path: &Path, required: bool) -> Self {
        self.builder = self.builder.add_source(File::from(path).required(required));
        self
    }
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn later_sources_override_earlier() {
        let app: App = ConfigLoader::new()
            .toml_str("name = \"svc\"\n[db]\nurl = \"a\"\npool = 1")
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
            .toml_str("name = \"svc\"\n[db]\nurl = \"a\"\npool = 1")
            .file("does/not/exist.toml")
            .load()
            .unwrap();
        assert_eq!(app.name, "svc");
    }

    #[test]
    fn missing_required_file_is_config_error() {
        let err = ConfigLoader::new()
            .required_file("does/not/exist.toml")
            .load::<App>()
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Config);
    }
}
