#[cfg(feature = "reqwest")]
mod reqwest;
#[cfg(feature = "serde_json")]
mod serde_json;
#[cfg(feature = "sqlx")]
mod sqlx;

use crate::Error;

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::internal("I/O error").with_source(err)
    }
}
