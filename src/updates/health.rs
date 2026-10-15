//! Authenticate loopback readiness without sending its private key over HTTP.
use super::{package, store::Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub version: String,
    pub pid: u32,
    pub database_id: String,
    pub frontend_version: Option<String>,
    pub update_id: Option<String>,
    pub ready: bool,
    pub managed: bool,
}
pub fn database_id(path: &std::path::Path) -> Result<String> {
    use sha2::Digest;
    let path = std::fs::canonicalize(path)
        .map_err(|e| format!("Cannot identify configured SQLite file: {e}"))?;
    Ok(package::hex(&sha2::Sha256::digest(
        path.to_string_lossy().as_bytes(),
    )))
}
pub fn proof(secret: &str, identity: &Identity) -> Result<String> {
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret.as_bytes());
    Ok(package::hex(
        ring::hmac::sign(
            &key,
            &serde_json::to_vec(identity).map_err(|e| e.to_string())?,
        )
        .as_ref(),
    ))
}
pub fn verify(secret: &str, identity: &Identity, value: &str) -> bool {
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret.as_bytes());
    match (serde_json::to_vec(identity), package::hex_decode(value)) {
        (Ok(bytes), Ok(signature)) => ring::hmac::verify(&key, &bytes, &signature).is_ok(),
        _ => false,
    }
}
