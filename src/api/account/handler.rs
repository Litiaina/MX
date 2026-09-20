use std::io::Cursor;

use axum::{Json, http::StatusCode, response::IntoResponse};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64_STANDARD};
use image::ImageFormat;
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

use crate::{
    api::account::model::{
        PasswordChangeRequest, ProfileUpdateRequest, RecoveryCodesResponse, SecurityReauthRequest,
        TotpConfirmRequest, TotpEnrollmentRequest, TotpEnrollmentResponse,
    },
    db::connector::{SqliteDatabaseError, with_sql_connection},
    middleware::{auth::Claims, totp::verify_totp},
    util::{
        authentication::{CredentialStatus, verify_user_credentials},
        password::{hash_password, validate_new_password},
        qr::generate_qr_image,
        randomizer::generate_random_base32,
        recovery::{generate_recovery_codes, recovery_code_digest},
    },
};

const TOTP_ENROLLMENT_TTL_SECONDS: i64 = 10 * 60;

type ApiError = (StatusCode, Json<Value>);

#[derive(Debug)]
enum OperationOutcome {
    Updated,
    UpdatedAndInvalidated,
    AlreadyDisabled,
    TotpAlreadyEnabled,
    NoTotp,
    NoPendingEnrollment,
    PendingEnrollmentExpired,
    InvalidEnrollmentCode,
    Credentials(CredentialStatus),
}

pub async fn update_profile(
    claims: Claims,
    Json(request): Json<ProfileUpdateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let name = request.name.map(|value| value.trim().to_string());
    let email = request.email.map(|value| value.trim().to_string());

    if name.as_deref().is_some_and(str::is_empty) || email.as_deref().is_some_and(str::is_empty) {
        return Err(bad_request("name and email cannot be empty"));
    }
    if name.is_none() && email.is_none() {
        return Err(bad_request("provide a name or email to update"));
    }
    if request.current_password.is_empty() {
        return Err(bad_request("current password is required"));
    }

    let uid = claims.uid;
    let invalidates_session = email.is_some();
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            let transaction = connection.unchecked_transaction()?;
            let status = verify_user_credentials(
                &transaction,
                &uid,
                &request.current_password,
                request.otp.as_deref(),
                request.recovery_code.as_deref(),
            )?;
            if status != CredentialStatus::Valid {
                return Ok(OperationOutcome::Credentials(status));
            }

            transaction.execute(
                r#"
                    UPDATE users
                    SET name = COALESCE(?1, name),
                        email = COALESCE(?2, email),
                        auth_version = auth_version + CASE WHEN ?2 IS NULL THEN 0 ELSE 1 END
                    WHERE uid = ?3
                "#,
                params![name, email, uid],
            )?;
            transaction.commit()?;
            Ok(if invalidates_session {
                OperationOutcome::UpdatedAndInvalidated
            } else {
                OperationOutcome::Updated
            })
        })
    })
    .await;

    match_operation(result, "profile updated")
}

pub async fn change_password(
    claims: Claims,
    Json(request): Json<PasswordChangeRequest>,
) -> Result<impl IntoResponse, ApiError> {
    validate_new_password(&request.new_password).map_err(bad_request)?;
    if request.current_password == request.new_password {
        return Err(bad_request(
            "new password must be different from the current password",
        ));
    }
    let new_hash = hash_password(&request.new_password).map_err(|_| internal_error())?;
    let uid = claims.uid;

    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            let transaction = connection.unchecked_transaction()?;
            let status = verify_user_credentials(
                &transaction,
                &uid,
                &request.current_password,
                request.otp.as_deref(),
                request.recovery_code.as_deref(),
            )?;
            if status != CredentialStatus::Valid {
                return Ok(OperationOutcome::Credentials(status));
            }

            transaction.execute(
                r#"
                    UPDATE users
                    SET password_hash = ?1,
                        password = '',
                        auth_version = auth_version + 1
                    WHERE uid = ?2
                "#,
                params![new_hash, uid],
            )?;
            transaction.commit()?;
            Ok(OperationOutcome::UpdatedAndInvalidated)
        })
    })
    .await;

    match_operation(result, "password changed")
}

pub async fn begin_totp_enrollment(
    claims: Claims,
    Json(request): Json<TotpEnrollmentRequest>,
) -> Result<Json<TotpEnrollmentResponse>, ApiError> {
    if request.password.is_empty() {
        return Err(bad_request("current password is required"));
    }

    let uid = claims.uid;
    let email = claims.email;
    let secret = generate_random_base32(20);
    let created_at = chrono::Utc::now().timestamp();
    let secret_for_db = secret.clone();

    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            let active: Option<String> = connection.query_row(
                "SELECT totp_secret FROM users WHERE uid = ?1",
                params![uid],
                |row| row.get(0),
            )?;
            if active.is_some() {
                return Ok(OperationOutcome::TotpAlreadyEnabled);
            }

            let status = verify_user_credentials(connection, &uid, &request.password, None, None)?;
            if status != CredentialStatus::Valid {
                return Ok(OperationOutcome::Credentials(status));
            }

            connection.execute(
                r#"
                    UPDATE users
                    SET totp_pending_secret = ?1,
                        totp_pending_created_at = ?2
                    WHERE uid = ?3
                "#,
                params![secret_for_db, created_at, uid],
            )?;
            Ok(OperationOutcome::Updated)
        })
    })
    .await;

    match result {
        Ok(Ok(OperationOutcome::Updated)) => {}
        Ok(Ok(OperationOutcome::TotpAlreadyEnabled)) => {
            return Err(conflict("two-factor authentication is already enabled"));
        }
        Ok(Ok(OperationOutcome::Credentials(status))) => return Err(credentials_error(status)),
        Ok(Ok(_)) => return Err(internal_error()),
        Ok(Err(error)) => {
            log_database_error("begin_totp_enrollment", &error);
            return Err(internal_error());
        }
        Err(error) => {
            crate::report_error!(format!("{error}"), "function", "begin_totp_enrollment()");
            return Err(internal_error());
        }
    }

    let label = percent_encode(&format!("MX:{email}"));
    let uri = format!("otpauth://totp/{label}?secret={secret}&issuer=MX");
    let image = generate_qr_image(uri.clone())
        .await
        .map_err(|_| internal_error())?;
    let mut buffer = Cursor::new(Vec::new());
    image
        .write_to(&mut buffer, ImageFormat::Png)
        .map_err(|_| internal_error())?;

    Ok(Json(TotpEnrollmentResponse {
        secret,
        otpauth_uri: uri,
        qr_code_data_url: format!(
            "data:image/png;base64,{}",
            BASE64_STANDARD.encode(buffer.into_inner())
        ),
        expires_in: TOTP_ENROLLMENT_TTL_SECONDS,
    }))
}

pub async fn confirm_totp_enrollment(
    claims: Claims,
    Json(request): Json<TotpConfirmRequest>,
) -> Result<Json<RecoveryCodesResponse>, ApiError> {
    if request.password.is_empty() || request.otp.trim().is_empty() {
        return Err(bad_request(
            "current password and authenticator code are required",
        ));
    }

    let codes = generate_recovery_codes();
    let digests = recovery_digests(&codes)?;
    let uid = claims.uid;
    let now = chrono::Utc::now().timestamp();

    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            let transaction = connection.unchecked_transaction()?;
            let status = verify_user_credentials(
                &transaction,
                &uid,
                &request.password,
                None,
                None,
            )?;
            if status != CredentialStatus::Valid {
                return Ok(OperationOutcome::Credentials(status));
            }

            let pending = transaction
                .query_row(
                    r#"
                        SELECT totp_pending_secret, totp_pending_created_at
                        FROM users
                        WHERE uid = ?1
                    "#,
                    params![uid],
                    |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<i64>>(1)?)),
                )
                .optional()?;
            let Some((Some(secret), Some(created_at))) = pending else {
                return Ok(OperationOutcome::NoPendingEnrollment);
            };

            if now.saturating_sub(created_at) > TOTP_ENROLLMENT_TTL_SECONDS {
                transaction.execute(
                    "UPDATE users SET totp_pending_secret = NULL, totp_pending_created_at = NULL WHERE uid = ?1",
                    params![uid],
                )?;
                transaction.commit()?;
                return Ok(OperationOutcome::PendingEnrollmentExpired);
            }
            if !verify_totp(&secret, request.otp.trim(), 1) {
                return Ok(OperationOutcome::InvalidEnrollmentCode);
            }

            transaction.execute("DELETE FROM user_recovery_codes WHERE user_uid = ?1", params![uid])?;
            for digest in digests {
                transaction.execute(
                    "INSERT INTO user_recovery_codes (user_uid, code_digest, created_at) VALUES (?1, ?2, ?3)",
                    params![uid, digest, now],
                )?;
            }
            transaction.execute(
                r#"
                    UPDATE users
                    SET totp_secret = ?1,
                        totp_pending_secret = NULL,
                        totp_pending_created_at = NULL,
                        auth_version = auth_version + 1
                    WHERE uid = ?2
                "#,
                params![secret, uid],
            )?;
            transaction.commit()?;
            Ok(OperationOutcome::UpdatedAndInvalidated)
        })
    })
    .await;

    match result {
        Ok(Ok(OperationOutcome::UpdatedAndInvalidated)) => Ok(Json(RecoveryCodesResponse {
            response: "two-factor authentication enabled",
            recovery_codes: codes,
            session_invalidated: true,
        })),
        Ok(Ok(OperationOutcome::NoPendingEnrollment)) => {
            Err(bad_request("start a new authenticator enrollment first"))
        }
        Ok(Ok(OperationOutcome::PendingEnrollmentExpired)) => {
            Err(bad_request("authenticator enrollment expired; start again"))
        }
        Ok(Ok(OperationOutcome::InvalidEnrollmentCode)) => Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "response": "invalid authenticator code" })),
        )),
        Ok(Ok(OperationOutcome::Credentials(status))) => Err(credentials_error(status)),
        Ok(Ok(_)) => Err(internal_error()),
        Ok(Err(error)) => {
            log_database_error("confirm_totp_enrollment", &error);
            Err(internal_error())
        }
        Err(error) => {
            crate::report_error!(format!("{error}"), "function", "confirm_totp_enrollment()");
            Err(internal_error())
        }
    }
}

pub async fn disable_totp(
    claims: Claims,
    Json(request): Json<SecurityReauthRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let uid = claims.uid;
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            let transaction = connection.unchecked_transaction()?;
            let active: Option<String> = transaction.query_row(
                "SELECT totp_secret FROM users WHERE uid = ?1",
                params![uid],
                |row| row.get(0),
            )?;
            if active.is_none() {
                transaction.execute(
                    "UPDATE users SET totp_pending_secret = NULL, totp_pending_created_at = NULL WHERE uid = ?1",
                    params![uid],
                )?;
                transaction.commit()?;
                return Ok(OperationOutcome::AlreadyDisabled);
            }

            let status = verify_user_credentials(
                &transaction,
                &uid,
                &request.password,
                request.otp.as_deref(),
                request.recovery_code.as_deref(),
            )?;
            if status != CredentialStatus::Valid {
                return Ok(OperationOutcome::Credentials(status));
            }

            transaction.execute("DELETE FROM user_recovery_codes WHERE user_uid = ?1", params![uid])?;
            transaction.execute(
                r#"
                    UPDATE users
                    SET totp_secret = NULL,
                        totp_pending_secret = NULL,
                        totp_pending_created_at = NULL,
                        auth_version = auth_version + 1
                    WHERE uid = ?1
                "#,
                params![uid],
            )?;
            transaction.commit()?;
            Ok(OperationOutcome::UpdatedAndInvalidated)
        })
    })
    .await;

    match_operation(result, "two-factor authentication disabled")
}

pub async fn regenerate_recovery_codes(
    claims: Claims,
    Json(request): Json<SecurityReauthRequest>,
) -> Result<Json<RecoveryCodesResponse>, ApiError> {
    let codes = generate_recovery_codes();
    let digests = recovery_digests(&codes)?;
    let uid = claims.uid;
    let now = chrono::Utc::now().timestamp();

    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            let transaction = connection.unchecked_transaction()?;
            let active: Option<String> = transaction.query_row(
                "SELECT totp_secret FROM users WHERE uid = ?1",
                params![uid],
                |row| row.get(0),
            )?;
            if active.is_none() {
                return Ok(OperationOutcome::NoTotp);
            }

            let status = verify_user_credentials(
                &transaction,
                &uid,
                &request.password,
                request.otp.as_deref(),
                request.recovery_code.as_deref(),
            )?;
            if status != CredentialStatus::Valid {
                return Ok(OperationOutcome::Credentials(status));
            }

            transaction.execute("DELETE FROM user_recovery_codes WHERE user_uid = ?1", params![uid])?;
            for digest in digests {
                transaction.execute(
                    "INSERT INTO user_recovery_codes (user_uid, code_digest, created_at) VALUES (?1, ?2, ?3)",
                    params![uid, digest, now],
                )?;
            }
            transaction.execute(
                "UPDATE users SET auth_version = auth_version + 1 WHERE uid = ?1",
                params![uid],
            )?;
            transaction.commit()?;
            Ok(OperationOutcome::UpdatedAndInvalidated)
        })
    })
    .await;

    match result {
        Ok(Ok(OperationOutcome::UpdatedAndInvalidated)) => Ok(Json(RecoveryCodesResponse {
            response: "recovery codes regenerated",
            recovery_codes: codes,
            session_invalidated: true,
        })),
        Ok(Ok(OperationOutcome::NoTotp)) => {
            Err(bad_request("enable two-factor authentication first"))
        }
        Ok(Ok(OperationOutcome::Credentials(status))) => Err(credentials_error(status)),
        Ok(Ok(_)) => Err(internal_error()),
        Ok(Err(error)) => {
            log_database_error("regenerate_recovery_codes", &error);
            Err(internal_error())
        }
        Err(error) => {
            crate::report_error!(
                format!("{error}"),
                "function",
                "regenerate_recovery_codes()"
            );
            Err(internal_error())
        }
    }
}

fn match_operation(
    result: Result<Result<OperationOutcome, SqliteDatabaseError>, tokio::task::JoinError>,
    success_message: &'static str,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    match result {
        Ok(Ok(OperationOutcome::Updated)) => Ok((
            StatusCode::OK,
            Json(json!({ "response": success_message, "session_invalidated": false })),
        )),
        Ok(Ok(OperationOutcome::UpdatedAndInvalidated)) => Ok((
            StatusCode::OK,
            Json(json!({ "response": success_message, "session_invalidated": true })),
        )),
        Ok(Ok(OperationOutcome::AlreadyDisabled)) => Ok((
            StatusCode::OK,
            Json(
                json!({ "response": "two-factor authentication is already disabled", "session_invalidated": false }),
            ),
        )),
        Ok(Ok(OperationOutcome::Credentials(status))) => Err(credentials_error(status)),
        Ok(Ok(_)) => Err(internal_error()),
        Ok(Err(SqliteDatabaseError::Sqlite(rusqlite::Error::SqliteFailure(error, _))))
            if error.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            Err(conflict("that email address is already in use"))
        }
        Ok(Err(error)) => {
            log_database_error(success_message, &error);
            Err(internal_error())
        }
        Err(error) => {
            crate::report_error!(format!("{error}"), "function", success_message);
            Err(internal_error())
        }
    }
}

fn recovery_digests(codes: &[String]) -> Result<Vec<String>, ApiError> {
    codes
        .iter()
        .map(|code| recovery_code_digest(code).ok_or_else(internal_error))
        .collect()
}

fn credentials_error(status: CredentialStatus) -> ApiError {
    match status {
        CredentialStatus::SecondFactorRequired => {
            bad_request("an authenticator or recovery code is required")
        }
        CredentialStatus::Invalid => (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "response": "invalid credentials" })),
        ),
        CredentialStatus::Valid => internal_error(),
    }
}

fn bad_request(message: impl Into<String>) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "response": message.into() })),
    )
}

fn conflict(message: &'static str) -> ApiError {
    (StatusCode::CONFLICT, Json(json!({ "response": message })))
}

fn internal_error() -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "response": "internal server error" })),
    )
}

fn log_database_error(context: &str, error: &SqliteDatabaseError) {
    crate::report_error!(format!("{error}"), "function", context);
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}
