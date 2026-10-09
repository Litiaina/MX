use std::io::Cursor;

use axum::{
    Json,
    body::Body,
    extract::{Multipart, Path},
    http::{
        HeaderValue, StatusCode,
        header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE},
    },
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64_STANDARD};
use image::ImageFormat;
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

use crate::{
    api::account::model::{
        PasswordChangeRequest, ProfileUpdateRequest, RecoveryCodesResponse, SecurityReauthRequest,
        TotpConfirmRequest, TotpEnrollmentRequest, TotpEnrollmentResponse,
    },
    api::{
        live::publish_live_event,
        mx::handler::{n1_access_token, n1_download, n1_soft_delete, n1_upload},
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

pub(crate) fn ensure_profile_photo_schema(
    connection: &rusqlite::Connection,
) -> rusqlite::Result<()> {
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS mx_user_profile_photos (
            user_uid   TEXT PRIMARY KEY NOT NULL,
            object_key TEXT NOT NULL UNIQUE,
            file_name  TEXT NOT NULL,
            mime_type  TEXT NOT NULL,
            size       INTEGER NOT NULL CHECK(size > 0),
            updated_at INTEGER NOT NULL,
            FOREIGN KEY(user_uid) REFERENCES users(uid) ON DELETE CASCADE
        );
        "#,
    )
}

fn profile_photo_type(mime_type: &str, file_name: &str) -> Option<(&'static str, &'static str)> {
    let mime = mime_type.trim().to_ascii_lowercase();
    let name = file_name.to_ascii_lowercase();
    match mime.as_str() {
        "image/png" => Some(("png", "image/png")),
        "image/jpeg" => Some(("jpg", "image/jpeg")),
        "image/webp" => Some(("webp", "image/webp")),
        "image/gif" => Some(("gif", "image/gif")),
        _ if name.ends_with(".png") => Some(("png", "image/png")),
        _ if name.ends_with(".jpg") || name.ends_with(".jpeg") => Some(("jpg", "image/jpeg")),
        _ if name.ends_with(".webp") => Some(("webp", "image/webp")),
        _ if name.ends_with(".gif") => Some(("gif", "image/gif")),
        _ => None,
    }
}

fn profile_photo_json(
    user_uid: &str,
    file_name: &str,
    mime_type: &str,
    size: i64,
    updated_at: i64,
) -> Value {
    json!({
        "exists": true,
        "file_name": file_name,
        "mime_type": mime_type,
        "size": size,
        "updated_at": updated_at,
        "url": format!("/mx/v1/account/profile-photo/{user_uid}?v={updated_at}")
    })
}

pub async fn upload_profile_photo(claims: Claims, mut multipart: Multipart) -> Response {
    let field = match multipart.next_field().await {
        Ok(Some(field)) => field,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"response":"select a profile image to upload"})),
            )
                .into_response();
        }
    };
    let file_name = field.file_name().unwrap_or("profile-photo").to_string();
    let supplied_type = field.content_type().unwrap_or("").to_string();
    let Some((extension, mime_type)) = profile_photo_type(&supplied_type, &file_name) else {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Json(json!({"response":"profile photo must be PNG, JPEG, WebP, or GIF"})),
        )
            .into_response();
    };
    let bytes = match field.bytes().await {
        Ok(bytes) if !bytes.is_empty() && bytes.len() <= 5 * 1024 * 1024 => bytes.to_vec(),
        Ok(_) => {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(json!({"response":"profile photo must be between 1 byte and 5 MiB"})),
            )
                .into_response();
        }
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"response":"profile photo upload could not be read"})),
            )
                .into_response();
        }
    };
    if image::load_from_memory(&bytes).is_err() {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Json(json!({"response":"the selected file is not a valid image"})),
        )
            .into_response();
    }

    let user_uid = claims.uid;
    let object_key = format!("__mx/profile-photos/{user_uid}/avatar.{extension}");
    let token = match n1_access_token().await {
        Ok(token) => token,
        Err(_) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"response":"N1 is unavailable; the profile photo was not changed"})),
            )
                .into_response();
        }
    };
    if n1_upload(&object_key, mime_type, bytes.clone(), &token)
        .await
        .is_err()
    {
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({"response":"N1 could not store the profile photo"})),
        )
            .into_response();
    }

    let now = chrono::Utc::now().timestamp_millis();
    let stored_uid = user_uid.clone();
    let stored_key = object_key.clone();
    let stored_name = file_name.clone();
    let stored_type = mime_type.to_string();
    let size = bytes.len() as i64;
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_profile_photo_schema(connection)?;
            let old_key = connection.query_row(
                "SELECT object_key FROM mx_user_profile_photos WHERE user_uid=?1",
                params![stored_uid],
                |row| row.get::<_, String>(0),
            ).optional()?;
            connection.execute(
                "INSERT INTO mx_user_profile_photos(user_uid,object_key,file_name,mime_type,size,updated_at) VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(user_uid) DO UPDATE SET object_key=excluded.object_key,file_name=excluded.file_name,mime_type=excluded.mime_type,size=excluded.size,updated_at=excluded.updated_at",
                params![stored_uid,stored_key,stored_name,stored_type,size,now],
            )?;
            Ok(old_key)
        })
    }).await;
    let old_key = match result {
        Ok(Ok(value)) => value,
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"response":"photo was stored but its metadata could not be saved"})),
            )
                .into_response();
        }
    };
    if let Some(old_key) = old_key.filter(|key| key != &object_key) {
        let _ = n1_soft_delete(&old_key).await;
    }
    publish_live_event(
        "profile.updated",
        Some(&user_uid),
        json!({"uid":user_uid,"profile_photo_updated_at":now}),
    );
    (
        StatusCode::OK,
        Json(profile_photo_json(
            &user_uid, &file_name, mime_type, size, now,
        )),
    )
        .into_response()
}

pub async fn get_profile_photo(_claims: Claims, Path(user_uid): Path<String>) -> Response {
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_profile_photo_schema(connection)?;
            connection.query_row(
                "SELECT object_key,file_name,mime_type,updated_at FROM mx_user_profile_photos WHERE user_uid=?1",
                params![user_uid],
                |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,i64>(3)?)),
            ).optional()
        })
    }).await;
    let Ok(Ok(Some((object_key, file_name, mime_type, updated_at)))) = result else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let bytes = match n1_download(&object_key, &file_name).await {
        Ok(bytes) => bytes,
        Err(_) => return StatusCode::BAD_GATEWAY.into_response(),
    };
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_str(&mime_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    let safe_name = file_name.replace('"', "_");
    if let Ok(value) = HeaderValue::from_str(&format!("inline; filename=\"{safe_name}\"")) {
        response.headers_mut().insert(CONTENT_DISPOSITION, value);
    }
    response.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=86400, immutable"),
    );
    if let Ok(value) = HeaderValue::from_str(&format!("\"{updated_at}\"")) {
        response.headers_mut().insert("etag", value);
    }
    response
}

pub async fn delete_profile_photo(claims: Claims) -> Response {
    let user_uid = claims.uid;
    let delete_uid = user_uid.clone();
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            ensure_profile_photo_schema(connection)?;
            let object_key = connection
                .query_row(
                    "SELECT object_key FROM mx_user_profile_photos WHERE user_uid=?1",
                    params![delete_uid],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            connection.execute(
                "DELETE FROM mx_user_profile_photos WHERE user_uid=?1",
                params![delete_uid],
            )?;
            Ok(object_key)
        })
    })
    .await;
    let object_key = match result {
        Ok(Ok(value)) => value,
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"response":"failed to remove profile photo"})),
            )
                .into_response();
        }
    };
    if let Some(object_key) = object_key {
        let _ = n1_soft_delete(&object_key).await;
    }
    publish_live_event(
        "profile.updated",
        Some(&user_uid),
        json!({"uid":user_uid,"profile_photo_updated_at":Value::Null}),
    );
    (
        StatusCode::OK,
        Json(json!({"response":"profile photo removed"})),
    )
        .into_response()
}

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

pub async fn cancel_totp_enrollment(
    claims: Claims,
    Json(request): Json<TotpEnrollmentRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if request.password.is_empty() {
        return Err(bad_request("current password is required"));
    }

    let uid = claims.uid;
    let result = tokio::task::spawn_blocking(move || {
        with_sql_connection(|connection| {
            let transaction = connection.unchecked_transaction()?;
            let active: Option<String> = transaction.query_row(
                "SELECT totp_secret FROM users WHERE uid = ?1",
                params![uid],
                |row| row.get(0),
            )?;
            if active.is_some() {
                return Ok(OperationOutcome::TotpAlreadyEnabled);
            }

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
            transaction.execute(
                "UPDATE users SET totp_pending_secret = NULL, totp_pending_created_at = NULL WHERE uid = ?1",
                params![uid],
            )?;
            transaction.commit()?;
            Ok(OperationOutcome::Updated)
        })
    })
    .await;

    match result {
        Ok(Ok(OperationOutcome::Updated)) => Ok((
            StatusCode::OK,
            Json(json!({
                "response":"pending authenticator enrollment cancelled",
                "session_invalidated":false
            })),
        )),
        Ok(Ok(OperationOutcome::TotpAlreadyEnabled)) => {
            Err(conflict("two-factor authentication is already enabled"))
        }
        Ok(Ok(OperationOutcome::Credentials(status))) => Err(credentials_error(status)),
        Ok(Ok(_)) => Err(internal_error()),
        Ok(Err(error)) => {
            log_database_error("cancel_totp_enrollment", &error);
            Err(internal_error())
        }
        Err(error) => {
            crate::report_error!(format!("{error}"), "function", "cancel_totp_enrollment()");
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
