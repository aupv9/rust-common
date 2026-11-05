use sqlx::error::ErrorKind as DbErrorKind;

use crate::{Error, ErrorKind};

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        let (kind, message) = match &err {
            sqlx::Error::RowNotFound => (ErrorKind::NotFound, "record not found"),
            sqlx::Error::Database(db) => match db.kind() {
                DbErrorKind::UniqueViolation => (ErrorKind::Conflict, "record already exists"),
                DbErrorKind::ForeignKeyViolation => {
                    (ErrorKind::Conflict, "referenced record constraint violated")
                }
                DbErrorKind::NotNullViolation | DbErrorKind::CheckViolation => {
                    (ErrorKind::InvalidInput, "record violates a constraint")
                }
                _ => (ErrorKind::Internal, "database error"),
            },
            sqlx::Error::PoolTimedOut => (ErrorKind::Timeout, "database pool timed out"),
            sqlx::Error::PoolClosed | sqlx::Error::Io(_) | sqlx::Error::Tls(_) => {
                (ErrorKind::Unavailable, "database unavailable")
            }
            _ => (ErrorKind::Internal, "database error"),
        };
        Error::new(kind, message).with_source(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_not_found_maps_to_not_found() {
        let err: Error = sqlx::Error::RowNotFound.into();
        assert_eq!(err.kind(), ErrorKind::NotFound);
    }

    #[test]
    fn pool_timeout_is_retryable() {
        let err: Error = sqlx::Error::PoolTimedOut.into();
        assert!(err.kind().is_retryable());
    }
}
