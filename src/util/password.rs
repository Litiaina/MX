use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use std::fmt::{Display, Formatter};

pub const MIN_PASSWORD_LENGTH: usize = 12;
pub const MAX_PASSWORD_LENGTH: usize = 1024;

#[derive(Debug)]
pub enum PasswordError {
    Hash,
}

impl Display for PasswordError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("password hashing failed")
    }
}

impl std::error::Error for PasswordError {}

pub fn validate_new_password(password: &str) -> Result<(), &'static str> {
    if password.chars().count() < MIN_PASSWORD_LENGTH {
        return Err("password must be at least 12 characters");
    }

    if password.len() > MAX_PASSWORD_LENGTH {
        return Err("password is too long");
    }

    Ok(())
}

pub fn hash_password(password: &str) -> Result<String, PasswordError> {
    let salt = SaltString::generate(&mut OsRng);

    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| PasswordError::Hash)
}

pub fn verify_password(password: &str, encoded_hash: &str) -> bool {
    let Ok(parsed_hash) = PasswordHash::new(encoded_hash) else {
        return false;
    };

    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::{hash_password, validate_new_password, verify_password};

    #[test]
    fn hashes_are_salted_and_verifiable() {
        let first = hash_password("correct horse battery staple").unwrap();
        let second = hash_password("correct horse battery staple").unwrap();

        assert_ne!(first, second);
        assert!(first.starts_with("$argon2id$"));
        assert!(verify_password("correct horse battery staple", &first));
        assert!(!verify_password("wrong password", &first));
    }

    #[test]
    fn new_password_policy_has_safe_bounds() {
        assert!(validate_new_password("short").is_err());
        assert!(validate_new_password("a secure passphrase").is_ok());
        assert!(validate_new_password(&"x".repeat(1025)).is_err());
    }
}
