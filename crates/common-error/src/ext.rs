use std::borrow::Cow;

use crate::{BoxError, Error, ErrorKind, Result};

pub trait ResultExt<T> {
    fn context(self, kind: ErrorKind, message: impl Into<Cow<'static, str>>) -> Result<T>;

    fn with_context<M, F>(self, kind: ErrorKind, f: F) -> Result<T>
    where
        M: Into<Cow<'static, str>>,
        F: FnOnce() -> M;
}

impl<T, E> ResultExt<T> for std::result::Result<T, E>
where
    E: Into<BoxError>,
{
    fn context(self, kind: ErrorKind, message: impl Into<Cow<'static, str>>) -> Result<T> {
        self.map_err(|e| Error::new(kind, message).with_source(e))
    }

    fn with_context<M, F>(self, kind: ErrorKind, f: F) -> Result<T>
    where
        M: Into<Cow<'static, str>>,
        F: FnOnce() -> M,
    {
        self.map_err(|e| Error::new(kind, f()).with_source(e))
    }
}

pub trait OptionExt<T> {
    fn or_not_found(self, message: impl Into<Cow<'static, str>>) -> Result<T>;
}

impl<T> OptionExt<T> for Option<T> {
    fn or_not_found(self, message: impl Into<Cow<'static, str>>) -> Result<T> {
        self.ok_or_else(|| Error::not_found(message))
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;

    #[test]
    fn context_wraps_source() {
        let res: std::result::Result<(), std::io::Error> = Err(std::io::Error::other("boom"));
        let err = res
            .context(ErrorKind::Unavailable, "upstream down")
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unavailable);
        assert_eq!(err.source().unwrap().to_string(), "boom");
    }

    #[test]
    fn with_context_is_lazy() {
        let res: std::result::Result<u8, std::io::Error> = Ok(1);
        let value = res
            .with_context(ErrorKind::Internal, || -> &str { unreachable!() })
            .unwrap();
        assert_eq!(value, 1);
    }

    #[test]
    fn none_becomes_not_found() {
        let err = None::<u8>.or_not_found("missing").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::NotFound);
    }
}
