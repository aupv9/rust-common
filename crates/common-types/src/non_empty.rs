use std::fmt;
use std::ops::Deref;

use common_error::Error;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct NonEmptyString(String);

impl NonEmptyString {
    pub fn new(value: impl Into<String>) -> Result<Self, Error> {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(Error::invalid_input("value must not be empty"));
        }
        if trimmed.len() == value.len() {
            Ok(Self(value))
        } else {
            Ok(Self(trimmed.to_owned()))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl TryFrom<String> for NonEmptyString {
    type Error = Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for NonEmptyString {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<NonEmptyString> for String {
    fn from(value: NonEmptyString) -> Self {
        value.0
    }
}

impl Deref for NonEmptyString {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for NonEmptyString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NonEmptyString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(feature = "sqlx")]
mod sqlx_impl {
    use sqlx::encode::IsNull;
    use sqlx::error::BoxDynError;
    use sqlx::postgres::{PgArgumentBuffer, PgHasArrayType, PgTypeInfo, PgValueRef};
    use sqlx::{Decode, Encode, Postgres, Type};

    use super::NonEmptyString;

    impl Type<Postgres> for NonEmptyString {
        fn type_info() -> PgTypeInfo {
            <String as Type<Postgres>>::type_info()
        }

        fn compatible(ty: &PgTypeInfo) -> bool {
            <String as Type<Postgres>>::compatible(ty)
        }
    }

    impl PgHasArrayType for NonEmptyString {
        fn array_type_info() -> PgTypeInfo {
            <String as PgHasArrayType>::array_type_info()
        }
    }

    impl Encode<'_, Postgres> for NonEmptyString {
        fn encode_by_ref(&self, buf: &mut PgArgumentBuffer) -> Result<IsNull, BoxDynError> {
            <String as Encode<Postgres>>::encode_by_ref(&self.0, buf)
        }
    }

    impl<'r> Decode<'r, Postgres> for NonEmptyString {
        fn decode(value: PgValueRef<'r>) -> Result<Self, BoxDynError> {
            let raw = <String as Decode<Postgres>>::decode(value)?;
            Ok(Self::new(raw)?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_blank() {
        assert!(NonEmptyString::new("   ").is_err());
        assert!(serde_json::from_str::<NonEmptyString>("\"\"").is_err());
    }

    #[test]
    fn trims_whitespace() {
        assert_eq!(NonEmptyString::new("  hi ").unwrap().as_str(), "hi");
    }
}
