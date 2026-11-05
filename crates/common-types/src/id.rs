use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

pub struct Id<T> {
    uuid: Uuid,
    marker: PhantomData<fn() -> T>,
}

impl<T> Id<T> {
    #[must_use]
    pub fn new() -> Self {
        Self::from_uuid(Uuid::now_v7())
    }

    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self {
            uuid,
            marker: PhantomData,
        }
    }

    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.uuid
    }

    #[must_use]
    pub const fn into_uuid(self) -> Uuid {
        self.uuid
    }
}

impl<T> Default for Id<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Id<T> {}

impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.uuid == other.uuid
    }
}

impl<T> Eq for Id<T> {}

impl<T> PartialOrd for Id<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Id<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.uuid.cmp(&other.uuid)
    }
}

impl<T> Hash for Id<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.uuid.hash(state);
    }
}

impl<T> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({})", self.uuid)
    }
}

impl<T> fmt::Display for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.uuid, f)
    }
}

impl<T> FromStr for Id<T> {
    type Err = common_error::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s)
            .map(Self::from_uuid)
            .map_err(|e| common_error::Error::invalid_input("invalid id").with_source(e))
    }
}

impl<T> From<Uuid> for Id<T> {
    fn from(uuid: Uuid) -> Self {
        Self::from_uuid(uuid)
    }
}

impl<T> From<Id<T>> for Uuid {
    fn from(id: Id<T>) -> Self {
        id.uuid
    }
}

impl<T> Serialize for Id<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.uuid.serialize(serializer)
    }
}

impl<'de, T> Deserialize<'de> for Id<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Uuid::deserialize(deserializer).map(Self::from_uuid)
    }
}

#[cfg(feature = "sqlx")]
mod sqlx_impl {
    use sqlx::encode::IsNull;
    use sqlx::error::BoxDynError;
    use sqlx::postgres::{PgArgumentBuffer, PgHasArrayType, PgTypeInfo, PgValueRef};
    use sqlx::{Decode, Encode, Postgres, Type};
    use uuid::Uuid;

    use super::Id;

    impl<T> Type<Postgres> for Id<T> {
        fn type_info() -> PgTypeInfo {
            <Uuid as Type<Postgres>>::type_info()
        }

        fn compatible(ty: &PgTypeInfo) -> bool {
            <Uuid as Type<Postgres>>::compatible(ty)
        }
    }

    impl<T> PgHasArrayType for Id<T> {
        fn array_type_info() -> PgTypeInfo {
            <Uuid as PgHasArrayType>::array_type_info()
        }
    }

    impl<T> Encode<'_, Postgres> for Id<T> {
        fn encode_by_ref(&self, buf: &mut PgArgumentBuffer) -> Result<IsNull, BoxDynError> {
            <Uuid as Encode<Postgres>>::encode_by_ref(&self.uuid, buf)
        }
    }

    impl<'r, T> Decode<'r, Postgres> for Id<T> {
        fn decode(value: PgValueRef<'r>) -> Result<Self, BoxDynError> {
            <Uuid as Decode<Postgres>>::decode(value).map(Self::from_uuid)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct User;

    #[test]
    fn new_ids_are_v7_and_time_ordered() {
        let a = Id::<User>::new();
        let b = Id::<User>::new();
        assert_eq!(a.as_uuid().get_version_num(), 7);
        assert!(a < b);
    }

    #[test]
    fn roundtrips_through_string_and_json() {
        let id = Id::<User>::new();
        assert_eq!(id.to_string().parse::<Id<User>>().unwrap(), id);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(serde_json::from_str::<Id<User>>(&json).unwrap(), id);
    }

    #[test]
    fn invalid_string_is_invalid_input() {
        let err = "nope".parse::<Id<User>>().unwrap_err();
        assert_eq!(err.kind(), common_error::ErrorKind::InvalidInput);
    }
}
