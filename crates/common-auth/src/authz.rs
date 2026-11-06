use std::borrow::Cow;
use std::fmt;

use common_error::{Error, Result};

use crate::Principal;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Requirement {
    Authenticated,
    Role(Cow<'static, str>),
    Scope(Cow<'static, str>),
    AnyOf(Vec<Requirement>),
    AllOf(Vec<Requirement>),
}

pub fn role(name: impl Into<Cow<'static, str>>) -> Requirement {
    Requirement::Role(name.into())
}

pub fn scope(name: impl Into<Cow<'static, str>>) -> Requirement {
    Requirement::Scope(name.into())
}

impl Requirement {
    pub fn any_of(requirements: impl IntoIterator<Item = Requirement>) -> Self {
        Self::AnyOf(requirements.into_iter().collect())
    }

    pub fn all_of(requirements: impl IntoIterator<Item = Requirement>) -> Self {
        Self::AllOf(requirements.into_iter().collect())
    }

    #[must_use]
    pub fn or(self, other: Requirement) -> Self {
        match self {
            Self::AnyOf(mut list) => {
                list.push(other);
                Self::AnyOf(list)
            }
            first => Self::AnyOf(vec![first, other]),
        }
    }

    #[must_use]
    pub fn and(self, other: Requirement) -> Self {
        match self {
            Self::AllOf(mut list) => {
                list.push(other);
                Self::AllOf(list)
            }
            first => Self::AllOf(vec![first, other]),
        }
    }

    #[must_use]
    pub fn is_satisfied_by(&self, principal: &Principal) -> bool {
        match self {
            Self::Authenticated => true,
            Self::Role(name) => principal.has_role(name),
            Self::Scope(name) => principal.has_scope(name),
            Self::AnyOf(list) => list.iter().any(|r| r.is_satisfied_by(principal)),
            Self::AllOf(list) => list.iter().all(|r| r.is_satisfied_by(principal)),
        }
    }

    pub fn check(&self, principal: &Principal) -> Result<()> {
        if self.is_satisfied_by(principal) {
            Ok(())
        } else {
            Err(Error::forbidden(format!("requires {self}")).with_code("PERMISSION_DENIED"))
        }
    }
}

impl fmt::Display for Requirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let join = |f: &mut fmt::Formatter<'_>, list: &[Requirement], sep: &str| {
            f.write_str("(")?;
            for (i, r) in list.iter().enumerate() {
                if i > 0 {
                    f.write_str(sep)?;
                }
                write!(f, "{r}")?;
            }
            f.write_str(")")
        };
        match self {
            Self::Authenticated => f.write_str("authentication"),
            Self::Role(name) => write!(f, "role `{name}`"),
            Self::Scope(name) => write!(f, "scope `{name}`"),
            Self::AnyOf(list) => join(f, list, " or "),
            Self::AllOf(list) => join(f, list, " and "),
        }
    }
}

pub trait Policy<R: ?Sized> {
    fn authorize(&self, principal: &Principal, resource: &R) -> Result<()>;
}

impl<R: ?Sized> Policy<R> for Requirement {
    fn authorize(&self, principal: &Principal, _resource: &R) -> Result<()> {
        self.check(principal)
    }
}

impl<R: ?Sized, F> Policy<R> for F
where
    F: Fn(&Principal, &R) -> Result<()>,
{
    fn authorize(&self, principal: &Principal, resource: &R) -> Result<()> {
        self(principal, resource)
    }
}

#[cfg(test)]
mod tests {
    use common_error::ErrorKind;

    use super::*;
    use crate::AuthMethod;

    fn reader() -> Principal {
        Principal::new("u1", AuthMethod::Jwt)
            .with_tenant("acme")
            .with_roles(["member"])
            .with_scopes(["orders:read"])
    }

    #[test]
    fn single_requirements() {
        let p = reader();
        assert!(Requirement::Authenticated.is_satisfied_by(&p));
        assert!(role("member").is_satisfied_by(&p));
        assert!(scope("orders:read").is_satisfied_by(&p));
        assert!(!scope("orders:write").is_satisfied_by(&p));
    }

    #[test]
    fn combinators() {
        let p = reader();
        assert!(role("admin").or(scope("orders:read")).is_satisfied_by(&p));
        assert!(
            !role("member")
                .and(scope("orders:write"))
                .is_satisfied_by(&p)
        );
        assert!(Requirement::all_of([role("member"), scope("orders:read")]).is_satisfied_by(&p));
        assert!(!Requirement::any_of([]).is_satisfied_by(&p));
    }

    #[test]
    fn denial_is_forbidden_with_readable_message() {
        let err = role("admin")
            .or(scope("orders:write"))
            .check(&reader())
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Forbidden);
        assert_eq!(err.code(), "PERMISSION_DENIED");
        assert_eq!(
            err.message(),
            "requires (role `admin` or scope `orders:write`)"
        );
    }

    struct Order {
        tenant: String,
        owner: String,
    }

    struct EditOrder;

    impl Policy<Order> for EditOrder {
        fn authorize(&self, principal: &Principal, order: &Order) -> Result<()> {
            principal.ensure_tenant(&order.tenant)?;
            if principal.has_role("admin") {
                return Ok(());
            }
            principal.ensure_subject(&order.owner)
        }
    }

    #[test]
    fn resource_policy() {
        let p = reader();
        let own = Order {
            tenant: "acme".into(),
            owner: "u1".into(),
        };
        let other = Order {
            tenant: "acme".into(),
            owner: "u2".into(),
        };
        let foreign = Order {
            tenant: "globex".into(),
            owner: "u1".into(),
        };
        assert!(p.authorize(&EditOrder, &own).is_ok());
        assert_eq!(
            p.authorize(&EditOrder, &other).unwrap_err().code(),
            "NOT_OWNER"
        );
        assert_eq!(
            p.authorize(&EditOrder, &foreign).unwrap_err().code(),
            "TENANT_MISMATCH"
        );
    }

    #[test]
    fn closures_and_requirements_are_policies() {
        let p = reader();
        let is_owner = |p: &Principal, order: &Order| p.ensure_subject(&order.owner);
        let order = Order {
            tenant: "acme".into(),
            owner: "u1".into(),
        };
        assert!(p.authorize(&is_owner, &order).is_ok());
        assert!(p.authorize(&scope("orders:read"), &order).is_ok());
    }
}
