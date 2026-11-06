use std::collections::BTreeSet;

use common_error::{Error, Result};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::{AuthMethod, Principal};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct ClaimMapping {
    pub subject: String,
    pub tenant: Option<String>,
    pub roles: Vec<String>,
    pub scopes: Vec<String>,
}

impl Default for ClaimMapping {
    fn default() -> Self {
        Self {
            subject: "sub".to_owned(),
            tenant: Some("tenant_id".to_owned()),
            roles: vec!["roles".to_owned()],
            scopes: vec!["scope".to_owned(), "scp".to_owned()],
        }
    }
}

impl ClaimMapping {
    pub fn to_principal(
        &self,
        claims: Map<String, Value>,
        method: AuthMethod,
    ) -> Result<Principal> {
        let subject = lookup(&claims, &self.subject)
            .and_then(scalar)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                Error::unauthorized("token has no subject").with_code("INVALID_TOKEN")
            })?;
        let tenant = self
            .tenant
            .as_deref()
            .and_then(|path| lookup(&claims, path))
            .and_then(scalar)
            .filter(|s| !s.is_empty());
        let roles = collect(&claims, &self.roles);
        let scopes = collect(&claims, &self.scopes);

        let principal = Principal::new(subject, method)
            .with_roles(roles)
            .with_scopes(scopes)
            .with_claims(claims);
        Ok(match tenant {
            Some(tenant) => principal.with_tenant(tenant),
            None => principal,
        })
    }
}

fn lookup<'a>(claims: &'a Map<String, Value>, path: &str) -> Option<&'a Value> {
    match path.strip_prefix('/') {
        Some(_) => {
            let mut segments = path.split('/').skip(1).map(unescape);
            let first = segments.next()?;
            segments.try_fold(claims.get(&first)?, |value, segment| match value {
                Value::Object(map) => map.get(&segment),
                Value::Array(items) => segment.parse::<usize>().ok().and_then(|i| items.get(i)),
                _ => None,
            })
        }
        None => claims.get(path),
    }
}

fn unescape(segment: &str) -> String {
    segment.replace("~1", "/").replace("~0", "~")
}

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn collect(claims: &Map<String, Value>, paths: &[String]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for value in paths.iter().filter_map(|p| lookup(claims, p)) {
        match value {
            Value::String(s) => out.extend(s.split_whitespace().map(str::to_owned)),
            Value::Array(items) => {
                out.extend(items.iter().filter_map(Value::as_str).map(str::to_owned));
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn claims(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(map) => map,
            _ => unreachable!(),
        }
    }

    #[test]
    fn default_mapping_reads_standard_claims() {
        let principal = ClaimMapping::default()
            .to_principal(
                claims(json!({
                    "sub": "u1",
                    "tenant_id": "acme",
                    "roles": ["admin", "member"],
                    "scope": "orders:read orders:write",
                })),
                AuthMethod::Jwt,
            )
            .unwrap();
        assert_eq!(principal.subject(), "u1");
        assert_eq!(principal.tenant(), Some("acme"));
        assert!(principal.has_role("admin"));
        assert!(principal.has_scope("orders:write"));
        assert_eq!(principal.claim("tenant_id"), Some(&json!("acme")));
    }

    #[test]
    fn scp_array_is_read_as_scopes() {
        let principal = ClaimMapping::default()
            .to_principal(
                claims(json!({"sub": "u1", "scp": ["a", "b"]})),
                AuthMethod::Jwt,
            )
            .unwrap();
        assert!(principal.has_scope("a") && principal.has_scope("b"));
        assert_eq!(principal.tenant(), None);
    }

    #[test]
    fn json_pointer_paths_reach_nested_and_url_claims() {
        let mapping = ClaimMapping {
            subject: "sub".into(),
            tenant: Some("/org/id".into()),
            roles: vec![
                "/realm_access/roles".into(),
                "/resource_access/api/roles".into(),
                "https://example.com/roles".into(),
            ],
            scopes: vec![],
        };
        let principal = mapping
            .to_principal(
                claims(json!({
                    "sub": "u1",
                    "org": {"id": 42},
                    "realm_access": {"roles": ["admin"]},
                    "resource_access": {"api": {"roles": ["writer"]}},
                    "https://example.com/roles": ["auditor"],
                })),
                AuthMethod::Jwt,
            )
            .unwrap();
        assert_eq!(principal.tenant(), Some("42"));
        assert!(principal.has_role("admin"));
        assert!(principal.has_role("writer"));
        assert!(principal.has_role("auditor"));
    }

    #[test]
    fn pointer_escapes_are_decoded() {
        let map = claims(json!({"a/b": {"c~d": "x"}}));
        assert_eq!(lookup(&map, "/a~1b/c~0d"), Some(&json!("x")));
    }

    #[test]
    fn missing_subject_is_unauthorized() {
        let err = ClaimMapping::default()
            .to_principal(claims(json!({"sub": ""})), AuthMethod::Jwt)
            .unwrap_err();
        assert_eq!(err.kind(), common_error::ErrorKind::Unauthorized);
    }
}
