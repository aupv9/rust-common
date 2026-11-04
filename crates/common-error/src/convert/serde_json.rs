use serde_json::error::Category;

use crate::{Error, ErrorKind};

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        let kind = match err.classify() {
            Category::Syntax | Category::Data | Category::Eof => ErrorKind::InvalidInput,
            Category::Io => ErrorKind::Internal,
        };
        Error::new(kind, "invalid JSON").with_source(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syntax_error_is_invalid_input() {
        let err: Error = serde_json::from_str::<u8>("{").unwrap_err().into();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }
}
