use base32::Alphabet;
use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;
use std::time::{SystemTime, UNIX_EPOCH};

pub type HmacSha1 = Hmac<Sha1>;

pub fn verify_totp(secret: &str, user_otp: &str, window: i64) -> bool {
    let secret = secret.trim().replace("=", "");
    let key = match base32::decode(Alphabet::Rfc4648 { padding: false }, &secret) {
        Some(k) => k,
        None => return false,
    };
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let time_step = 30;
    let counter = time / time_step;

    for i in -window..=window {
        let time_counter = counter + i;
        let otp = generate_totp_from_counter(&key, time_counter);
        if otp == user_otp {
            return true;
        }
    }

    false
}

pub fn generate_totp_from_counter(key: &[u8], counter: i64) -> String {
    let mut msg = [0u8; 8];
    msg.copy_from_slice(&(counter as u64).to_be_bytes());

    let mac_result = HmacSha1::new_from_slice(key);
    if mac_result.is_err() {
        tracing::error!("Failed to create HMAC from key");
        return "000000".to_string();
    }

    let mut mac = mac_result.unwrap();
    mac.update(&msg);
    let hash = mac.finalize().into_bytes();

    let offset = (hash[hash.len() - 1] & 0xf) as usize;
    let part = &hash[offset..offset + 4];
    let truncated = ((u32::from(part[0]) & 0x7f) << 24)
        | (u32::from(part[1]) << 16)
        | (u32::from(part[2]) << 8)
        | u32::from(part[3]);

    let code = truncated % 1_000_000;
    let result = format!("{:06}", code);

    result
}
