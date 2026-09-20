use data_encoding::{BASE32, HEXLOWER};
use rand::Rng;
use sha2::{Digest, Sha256};

pub const RECOVERY_CODE_COUNT: usize = 10;

pub fn generate_recovery_codes() -> Vec<String> {
    (0..RECOVERY_CODE_COUNT)
        .map(|_| {
            let mut bytes = [0u8; 12];
            rand::rng().fill_bytes(&mut bytes);
            let encoded = BASE32.encode(&bytes).replace('=', "");
            encoded
                .as_bytes()
                .chunks(4)
                .map(|chunk| std::str::from_utf8(chunk).unwrap_or_default())
                .collect::<Vec<_>>()
                .join("-")
        })
        .collect()
}

pub fn recovery_code_digest(code: &str) -> Option<String> {
    if code.len() > 64 {
        return None;
    }

    let normalized = code
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .map(|character| character.to_ascii_uppercase())
        .collect::<String>();

    if normalized.len() != 20 {
        return None;
    }

    let digest = Sha256::digest(normalized.as_bytes());
    Some(HEXLOWER.encode(&digest))
}

#[cfg(test)]
mod tests {
    use super::{RECOVERY_CODE_COUNT, generate_recovery_codes, recovery_code_digest};
    use std::collections::HashSet;

    #[test]
    fn recovery_codes_are_unique_and_normalized_before_hashing() {
        let codes = generate_recovery_codes();
        assert_eq!(codes.len(), RECOVERY_CODE_COUNT);
        assert_eq!(
            codes.iter().collect::<HashSet<_>>().len(),
            RECOVERY_CODE_COUNT
        );

        let code = &codes[0];
        assert_eq!(
            recovery_code_digest(code),
            recovery_code_digest(&code.to_ascii_lowercase().replace('-', " "))
        );
    }
}
