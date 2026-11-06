use argon2::password_hash::Error as HashError;
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use common_error::{Error, Result};
use common_types::Secret;

pub async fn hash_password(password: Secret<String>) -> Result<String> {
    tokio::task::spawn_blocking(move || hash_password_blocking(password.expose()))
        .await
        .map_err(|e| Error::internal("password hashing task failed").with_source(e))?
}

pub async fn verify_password(password: Secret<String>, hash: String) -> Result<bool> {
    tokio::task::spawn_blocking(move || verify_password_blocking(password.expose(), &hash))
        .await
        .map_err(|e| Error::internal("password verification task failed").with_source(e))?
}

pub fn hash_password_blocking(password: &str) -> Result<String> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|e| Error::internal("failed to hash password").with_source(e))
}

pub fn verify_password_blocking(password: &str, hash: &str) -> Result<bool> {
    match Argon2::default().verify_password(password.as_bytes(), hash) {
        Ok(()) => Ok(true),
        Err(HashError::PasswordInvalid) => Ok(false),
        Err(e) => Err(Error::internal("stored password hash is invalid").with_source(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hash_and_verify() {
        let hash = hash_password(Secret::new("hunter2".into())).await.unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(
            verify_password(Secret::new("hunter2".into()), hash.clone())
                .await
                .unwrap()
        );
        assert!(
            !verify_password(Secret::new("hunter3".into()), hash)
                .await
                .unwrap()
        );
    }

    #[test]
    fn malformed_hash_is_an_error() {
        assert!(verify_password_blocking("pw", "not-a-hash").is_err());
    }
}
