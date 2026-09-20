use rusqlite::{Connection, OptionalExtension, params};

use crate::{
    middleware::totp::verify_totp,
    util::{
        password::{MAX_PASSWORD_LENGTH, verify_password},
        recovery::recovery_code_digest,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialStatus {
    Valid,
    Invalid,
    SecondFactorRequired,
}

/// Verifies a user's password and, when enabled, TOTP or a single-use recovery
/// code. A supplied recovery code is deleted in the same connection/transaction
/// before success is returned.
pub fn verify_user_credentials(
    connection: &Connection,
    uid: &str,
    password: &str,
    otp: Option<&str>,
    recovery_code: Option<&str>,
) -> rusqlite::Result<CredentialStatus> {
    if password.is_empty() || password.len() > MAX_PASSWORD_LENGTH {
        return Ok(CredentialStatus::Invalid);
    }

    let row = connection
        .query_row(
            r#"
                SELECT password_hash, totp_secret
                FROM users
                WHERE uid = ?1
                LIMIT 1
            "#,
            params![uid],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            },
        )
        .optional()?;

    let Some((password_hash, totp_secret)) = row else {
        return Ok(CredentialStatus::Invalid);
    };

    let password_valid = password_hash
        .as_deref()
        .filter(|hash| !hash.is_empty())
        .map(|hash| verify_password(password, hash))
        .unwrap_or(false);

    if !password_valid {
        return Ok(CredentialStatus::Invalid);
    }

    let Some(totp_secret) = totp_secret else {
        return Ok(CredentialStatus::Valid);
    };

    if otp
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some_and(|value| verify_totp(&totp_secret, value, 1))
    {
        return Ok(CredentialStatus::Valid);
    }

    if let Some(digest) = recovery_code.and_then(recovery_code_digest) {
        let consumed = connection.execute(
            "DELETE FROM user_recovery_codes WHERE user_uid = ?1 AND code_digest = ?2",
            params![uid, digest],
        )?;

        if consumed == 1 {
            return Ok(CredentialStatus::Valid);
        }
    }

    if otp.map(str::trim).unwrap_or_default().is_empty()
        && recovery_code.map(str::trim).unwrap_or_default().is_empty()
    {
        Ok(CredentialStatus::SecondFactorRequired)
    } else {
        Ok(CredentialStatus::Invalid)
    }
}

#[cfg(test)]
mod tests {
    use super::{CredentialStatus, verify_user_credentials};
    use crate::util::{password::hash_password, recovery::recovery_code_digest};
    use rusqlite::{Connection, params};

    fn test_connection(totp_secret: Option<&str>) -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                r#"
                    CREATE TABLE users (
                        uid TEXT PRIMARY KEY,
                        password TEXT NOT NULL DEFAULT '',
                        password_hash TEXT,
                        totp_secret TEXT
                    );
                    CREATE TABLE user_recovery_codes (
                        user_uid TEXT NOT NULL,
                        code_digest TEXT NOT NULL UNIQUE
                    );
                "#,
            )
            .unwrap();
        let hash = hash_password("correct horse battery staple").unwrap();
        connection
            .execute(
                "INSERT INTO users (uid, password_hash, totp_secret) VALUES ('user-1', ?1, ?2)",
                params![hash, totp_secret],
            )
            .unwrap();
        connection
    }

    #[test]
    fn verifies_argon2id_passwords() {
        let connection = test_connection(None);
        assert_eq!(
            verify_user_credentials(
                &connection,
                "user-1",
                "correct horse battery staple",
                None,
                None,
            )
            .unwrap(),
            CredentialStatus::Valid
        );
        assert_eq!(
            verify_user_credentials(&connection, "user-1", "wrong password", None, None).unwrap(),
            CredentialStatus::Invalid
        );
    }

    #[test]
    fn recovery_codes_are_consumed_once() {
        let connection = test_connection(Some("JBSWY3DPEHPK3PXP"));
        let code = "ABCD-EFGH-IJKL-MNOP-QRST";
        connection
            .execute(
                "INSERT INTO user_recovery_codes (user_uid, code_digest) VALUES ('user-1', ?1)",
                params![recovery_code_digest(code).unwrap()],
            )
            .unwrap();

        assert_eq!(
            verify_user_credentials(
                &connection,
                "user-1",
                "correct horse battery staple",
                None,
                Some(code),
            )
            .unwrap(),
            CredentialStatus::Valid
        );
        assert_eq!(
            verify_user_credentials(
                &connection,
                "user-1",
                "correct horse battery staple",
                None,
                Some(code),
            )
            .unwrap(),
            CredentialStatus::Invalid
        );
    }
}
