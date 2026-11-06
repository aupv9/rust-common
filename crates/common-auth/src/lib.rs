mod api_key;
mod authenticator;
mod authz;
mod claims;
#[cfg(feature = "issuer")]
mod issuer;
#[cfg(feature = "jwks")]
mod jwks;
mod jwt;
#[cfg(feature = "password")]
mod password;
mod principal;

pub use api_key::{ApiKeyConfig, ApiKeyEntry, ApiKeyVerifier, generate_api_key, hash_api_key};
pub use authenticator::{AuthConfig, Authenticator, Credential};
pub use authz::{Policy, Requirement, role, scope};
pub use claims::ClaimMapping;
#[cfg(feature = "issuer")]
pub use issuer::{IssuedToken, IssuerConfig, JwtIssuer, SigningKey};
pub use jsonwebtoken::Algorithm;
pub use jsonwebtoken::jwk::JwkSet;
pub use jwt::{JwtConfig, JwtVerifier, KeySource};
#[cfg(feature = "password")]
pub use password::{
    hash_password, hash_password_blocking, verify_password, verify_password_blocking,
};
pub use principal::{AuthMethod, Principal};
