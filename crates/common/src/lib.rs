//! Facade crate: depend on `common` and enable only the features you need.
//!
//! | Feature     | Module        | Crate              |
//! |-------------|---------------|--------------------|
//! | *(always)*  | [`error`]     | `common-error`     |
//! | `config`    | `config`      | `common-config`    |
//! | `telemetry` | `telemetry`   | `common-telemetry` |
//! | `utils`     | `utils`       | `common-utils`     |
//! | `full`      | all of the above                 ||

pub use common_error as error;
pub use common_error::{Error, ErrorKind, Result};

#[cfg(feature = "config")]
pub use common_config as config;

#[cfg(feature = "telemetry")]
pub use common_telemetry as telemetry;

#[cfg(feature = "utils")]
pub use common_utils as utils;
