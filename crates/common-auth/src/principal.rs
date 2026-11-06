use std::collections::BTreeSet;

use common_error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::authz::{Policy, Requirement};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AuthMethod {
    Jwt,
    ApiKey,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Principal {
    subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tenant: Option<String>,
    #[serde(default)]
    roles: BTreeSet<String>,
    #[serde(default)]
    scopes: BTreeSet<String>,
    method: AuthMethod,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    claims: Map<String, Value>,
}

impl Principal {
    pub fn new(subject: impl Into<String>, method: AuthMethod) -> Self {
        Self {
            subject: subject.into(),
            tenant: None,
            roles: BTreeSet::new(),
            scopes: BTreeSet::new(),
            method,
            claims: Map::new(),
        }
    }

    #[must_use]
    pub fn with_tenant(mut self, tenant: impl Into<String>) -> Self {
        self.tenant = Some(tenant.into());
        self
    }

    #[must_use]
    pub fn with_roles<I, S>(mut self, roles: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.roles.extend(roles.into_iter().map(Into::into));
        self
    }

    #[must_use]
    pub fn with_scopes<I, S>(mut self, scopes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.scopes.extend(scopes.into_iter().map(Into::into));
        self
    }

    #[must_use]
    pub fn with_claims(mut self, claims: Map<String, Value>) -> Self {
        self.claims = claims;
        self
    }

    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    #[must_use]
    pub fn tenant(&self) -> Option<&str> {
        self.tenant.as_deref()
    }

    #[must_use]
    pub fn roles(&self) -> &BTreeSet<String> {
        &self.roles
    }

    #[must_use]
    pub fn scopes(&self) -> &BTreeSet<String> {
        &self.scopes
    }

    #[must_use]
    pub fn method(&self) -> AuthMethod {
        self.method
    }

    #[must_use]
    pub fn claims(&self) -> &Map<String, Value> {
        &self.claims
    }

    #[must_use]
    pub fn claim(&self, name: &str) -> Option<&Value> {
        self.claims.get(name)
    }

    #[must_use]
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.contains(role)
    }

    #[must_use]
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.contains(scope)
    }

    pub fn require(&self, requirement: &Requirement) -> Result<()> {
        requirement.check(self)
    }

    pub fn authorize<R: ?Sized>(&self, policy: &impl Policy<R>, resource: &R) -> Result<()> {
        policy.authorize(self, resource)
    }

    pub fn require_tenant(&self) -> Result<&str> {
        self.tenant
            .as_deref()
            .ok_or_else(|| Error::forbidden("tenant required").with_code("TENANT_REQUIRED"))
    }

    pub fn ensure_tenant(&self, tenant: &str) -> Result<()> {
        if self.require_tenant()? == tenant {
            Ok(())
        } else {
            Err(Error::forbidden("resource belongs to another tenant").with_code("TENANT_MISMATCH"))
        }
    }

    pub fn ensure_subject(&self, subject: &str) -> Result<()> {
        if self.subject == subject {
            Ok(())
        } else {
            Err(Error::forbidden("not the owner of this resource").with_code("NOT_OWNER"))
        }
    }
}

#[cfg(test)]
mod tests {
    use common_error::ErrorKind;

    use super::*;

    fn alice() -> Principal {
        Principal::new("alice", AuthMethod::Jwt)
            .with_tenant("acme")
            .with_roles(["admin"])
            .with_scopes(["orders:read", "orders:write"])
    }

    #[test]
    fn exposes_roles_and_scopes() {
        let p = alice();
        assert!(p.has_role("admin"));
        assert!(!p.has_role("owner"));
        assert!(p.has_scope("orders:write"));
        assert_eq!(p.tenant(), Some("acme"));
    }

    #[test]
    fn tenant_checks() {
        let p = alice();
        assert!(p.ensure_tenant("acme").is_ok());
        let err = p.ensure_tenant("globex").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Forbidden);
        assert_eq!(err.code(), "TENANT_MISMATCH");

        let err = Principal::new("bob", AuthMethod::ApiKey)
            .require_tenant()
            .unwrap_err();
        assert_eq!(err.code(), "TENANT_REQUIRED");
    }

    #[test]
    fn subject_check() {
        let p = alice();
        assert!(p.ensure_subject("alice").is_ok());
        assert_eq!(p.ensure_subject("bob").unwrap_err().code(), "NOT_OWNER");
    }

    #[test]
    fn serde_roundtrip() {
        let p = alice();
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<Principal>(&json).unwrap(), p);
    }
}
