# common-auth

Authentication and authorization for backend services. Part of [rust-common](../../README.md).

The crate covers the parts that are the same in every service: verifying credentials, turning them into a `Principal`, and checking roles, scopes, tenants and ownership. Login flows, refresh tokens, sessions and user/role storage stay in your service.

```rust
use common_auth::{AuthConfig, Authenticator, Credential, scope};

let authenticator = Authenticator::from_config(&cfg.auth)?;
let principal = authenticator.authenticate(Credential::Bearer(token)).await?;

principal.require(&scope("orders:write"))?;
principal.ensure_tenant(&order.tenant_id)?;
```

With axum, enable the `auth` feature of [`common-axum`](../common-axum#authentication) and use `with_auth`, the `Auth` extractor and the `require` layer.

## Principal

| Field | Source (JWT, default mapping) | Source (API key) |
|---|---|---|
| `subject()` | `sub` | `subject` |
| `tenant()` | `tenant_id` | `tenant` |
| `roles()` | `roles` | `roles` |
| `scopes()` | `scope` (space-separated) or `scp` (array) | `scopes` |
| `method()` | `AuthMethod::Jwt` | `AuthMethod::ApiKey` |
| `claims()` | every claim in the token | `api_key_id` |

## Authentication

### JWT from an identity provider (JWKS)

```toml
[auth.jwt]
issuers = ["https://idp.example.com/realms/main"]
audiences = ["orders-api"]
algorithms = ["RS256"]
leeway_secs = 30
keys = { type = "jwks", url = "https://idp.example.com/realms/main/protocol/openid-connect/certs" }

[auth.jwt.claims]
tenant = "/org/id"
roles = ["/realm_access/roles", "/resource_access/orders-api/roles"]
```

- `issuers`, `audiences` and `algorithms` are required. The verifier always checks the signature, `exp`, `nbf`, `iss` and `aud`, and rejects any algorithm that is not on the list.
- Claim paths are either a top-level claim name (`roles`, `https://example.com/roles`) or a JSON pointer starting with `/` for nested claims.
- Keys are cached for `cache_secs` (300). A token with an unknown `kid` triggers a refetch, at most once every `min_refresh_secs` (30), so a stream of forged tokens cannot flood the IdP. Fetches are single-flight.
- If the IdP is down, cached keys keep working. If no usable key is available, the request fails with 503 `JWKS_UNAVAILABLE`, not 401.
- Only asymmetric signing keys are taken from the JWKS. `oct` keys and keys with `use: enc` are ignored.
- Call `JwtVerifier::preload()` at startup to fail fast when the JWKS is unreachable.

### JWT with a static key

```toml
keys = { type = "secret", secret = "at-least-32-bytes-of-random-data!!" }   # HS256/384/512
keys = { type = "public_key_pem", pem = "-----BEGIN PUBLIC KEY-----\n..." } # RS*, PS*, ES*, EdDSA
```

### API keys

```toml
[[auth.api_keys.keys]]
id = "billing-worker"
hash = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
subject = "svc:billing"
scopes = ["invoices:write"]
```

Only the SHA-256 hash of each key is stored in configuration. `generate_api_key("sk")` returns a new random key (`sk_...`, 256 bits) and its hash. Comparisons are constant-time.

### Errors

| Situation | Kind | Code |
|---|---|---|
| Expired token | `Unauthorized` | `TOKEN_EXPIRED` |
| Bad signature, issuer, audience, algorithm, format or unknown `kid` | `Unauthorized` | `INVALID_TOKEN` |
| Wrong API key | `Unauthorized` | `INVALID_API_KEY` |
| Credential type not configured | `Unauthorized` | `UNSUPPORTED_CREDENTIAL` |
| JWKS unreachable and no cached key | `Unavailable` | `JWKS_UNAVAILABLE` |

The client only sees "invalid token". The underlying reason is kept as the error source for logs.

## Issuing tokens (`issuer` feature)

For services that run their own login:

```toml
[issuer]
issuer = "https://orders.example.com"
audiences = ["orders-api"]
algorithm = "EdDSA"
key = { type = "private_key_pem", pem = "-----BEGIN PRIVATE KEY-----\n..." }
key_id = "2026-11"
ttl_secs = 900
```

```rust
let issuer = JwtIssuer::new(&cfg.issuer)?;
let principal = Principal::new(user.id.to_string(), AuthMethod::Jwt)
    .with_tenant(user.tenant_id.to_string())
    .with_roles(user.roles)
    .with_scopes(["orders:read"]);
let token = issuer.issue(&principal)?;   // serializes as {access_token, token_type, expires_in, expires_at}
let jwks = issuer.jwks()?;              // serve at /.well-known/jwks.json for other services
```

Issued tokens use the claim names of the default mapping (`sub`, `tenant_id`, `roles`, `scope`) plus `iss`, `aud`, `iat`, `nbf`, `exp` and a UUID v7 `jti`, so a `JwtVerifier` with the default `ClaimMapping` reads them back. Other claims from `Principal::with_claims` are passed through.

## Authorization

`Requirement` covers roles and scopes:

```rust
use common_auth::{Requirement, role, scope};

principal.require(&scope("orders:write"))?;
principal.require(&role("admin").or(scope("orders:admin")))?;
principal.require(&Requirement::all_of([role("member"), scope("orders:read")]))?;
```

A failed check returns `Forbidden` with code `PERMISSION_DENIED` and a message such as `requires (role `admin` or scope `orders:admin`)`.

For rules that depend on the resource, implement `Policy<R>` (closures `Fn(&Principal, &R) -> Result<()>` work too). Load the resource first, then check:

```rust
struct EditOrder;

impl Policy<Order> for EditOrder {
    fn authorize(&self, p: &Principal, order: &Order) -> Result<()> {
        p.ensure_tenant(&order.tenant_id)?;      // TENANT_MISMATCH
        if p.has_role("admin") {
            return Ok(());
        }
        p.ensure_subject(&order.owner_id)        // NOT_OWNER
    }
}

let order = repo.get(id).await?;
principal.authorize(&EditOrder, &order)?;
```

## Passwords (`password` feature)

`hash_password` / `verify_password` use Argon2id with the OWASP-recommended defaults and run on the blocking thread pool. `*_blocking` variants are available outside an async context. `verify_password` returns `Ok(false)` for a wrong password and `Err` only for a malformed stored hash.

## Features

| Feature | Default | Enables |
|---|---|---|
| `jwks` | ✓ | `KeySource::Jwks`, pulls in `common-http-client` |
| `issuer` | | `JwtIssuer`, `IssuerConfig` |
| `password` | | Argon2id password hashing |

Cryptography uses aws-lc-rs, the same provider as the rest of the workspace.
