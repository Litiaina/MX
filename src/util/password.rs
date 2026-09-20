use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::SaltString,
};
use rand::Rng;

pub const MIN_PASSWORD_LENGTH: usize = 12;

pub fn validate_new_password(password: &str) -> Result<(), String> {
    if password.len() < MIN_PASSWORD_LENGTH {
        return Err(format!(
            "password must be at least {} characters long",
            MIN_PASSWORD_LENGTH
        ));
    }

    Ok(())
}

pub fn hash_password(password: &str) -> Result<String, String> {
    validate_new_password(password)?;

    let mut salt_bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut salt_bytes);

    let salt = SaltString::encode_b64(&salt_bytes)
        .map_err(|error| format!("failed to encode password salt: {error}"))?;

    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| format!("failed to hash password: {error}"))
}

pub fn verify_password(stored_password: &str, candidate_password: &str) -> bool {
    match PasswordHash::new(stored_password) {
        Ok(password_hash) => Argon2::default()
            .verify_password(candidate_password.as_bytes(), &password_hash)
            .is_ok(),

        /*
         * Compatibility fallback for an MX 1.0 database that has not yet
         * completed the startup migration. initialize_sql_db() rewrites
         * legacy plaintext values to Argon2id before the server starts.
         */
        Err(_) => stored_password == candidate_password,
    }
}

pub fn is_password_hash(value: &str) -> bool {
    PasswordHash::new(value).is_ok()
}
